use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

const CG_ROOT: &str = "/sys/fs/cgroup";

pub struct Limits {
    pub cpus: Option<u32>,
    pub mem_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct Handle {
    pub pid: Option<u32>,
    pub cgroup: Option<PathBuf>,
    pub container: Option<String>,
}

pub struct Cell {
    pub child: Child,
    pub handle: Handle,
}

pub fn cgroups_usable() -> bool {
    rustix::process::geteuid().is_root()
        && Path::new(CG_ROOT).join("cgroup.controllers").is_file()
}

fn make_cgroup(name: &str, lim: &Limits) -> Option<PathBuf> {
    if !cgroups_usable() {
        return None;
    }
    let base = Path::new(CG_ROOT).join("exzet");
    fs::create_dir_all(&base).ok()?;
    let _ = fs::write(Path::new(CG_ROOT).join("cgroup.subtree_control"), "+cpu +memory");
    let _ = fs::write(base.join("cgroup.subtree_control"), "+cpu +memory");
    let cg = base.join(name);
    fs::create_dir_all(&cg).ok()?;
    if let Some(c) = lim.cpus {
        let _ = fs::write(cg.join("cpu.max"), format!("{} 100000", c as u64 * 100_000));
    }
    if let Some(m) = lim.mem_bytes {
        let _ = fs::write(cg.join("memory.max"), m.to_string());
    }
    Some(cg)
}

fn io(cmd: &mut Command, pipe: bool) {
    if pipe {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
}

pub fn spawn(
    job: &str,
    rank: u32,
    world: u32,
    main_ip: &str,
    script: &str,
    dir: &Path,
    lim: &Limits,
    pipe: bool,
) -> Result<Cell> {
    let cgroup = make_cgroup(&format!("{job}-{rank}"), lim);
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-euc")
        .arg(script)
        .current_dir(dir)
        .env("EXZET_JOB", job)
        .env("EXZET_RANK", rank.to_string())
        .env("EXZET_WORLD", world.to_string())
        .env("EXZET_MAIN", main_ip)
        .stdin(Stdio::null())
        .process_group(0)
        .kill_on_drop(true);
    io(&mut cmd, pipe);
    let child = cmd.spawn().context("spawning cell")?;
    let pid = child.id();
    if let (Some(cg), Some(pid)) = (&cgroup, pid) {
        let _ = fs::write(cg.join("cgroup.procs"), pid.to_string());
    }
    Ok(Cell {
        handle: Handle { pid, cgroup, container: None },
        child,
    })
}

pub fn spawn_docker(
    job: &str,
    rank: u32,
    world: u32,
    main_ip: &str,
    script: &str,
    dir: &Path,
    lim: &Limits,
    image: &str,
    pipe: bool,
) -> Result<Cell> {
    let name = format!("exzet-{job}-{rank}");
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "--network", "host", "--name", &name])
        .arg("-v")
        .arg(format!("{}:/work", dir.display()))
        .args(["-w", "/work", "--entrypoint", "/bin/sh"])
        .arg("-e")
        .arg(format!("EXZET_JOB={job}"))
        .arg("-e")
        .arg(format!("EXZET_RANK={rank}"))
        .arg("-e")
        .arg(format!("EXZET_WORLD={world}"))
        .arg("-e")
        .arg(format!("EXZET_MAIN={main_ip}"));
    if let Some(c) = lim.cpus {
        cmd.arg("--cpus").arg(c.to_string());
    }
    if let Some(m) = lim.mem_bytes {
        cmd.arg("--memory").arg(m.to_string());
    }
    cmd.arg(image)
        .args(["-euc", script])
        .stdin(Stdio::null())
        .process_group(0)
        .kill_on_drop(true);
    io(&mut cmd, pipe);
    let child = cmd.spawn().context("spawning docker cell")?;
    let pid = child.id();
    Ok(Cell {
        handle: Handle { pid, cgroup: None, container: Some(name) },
        child,
    })
}

pub fn kill_tree(h: &Handle) {
    if let Some(name) = &h.container {
        let _ = std::process::Command::new("docker")
            .args(["kill", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        return;
    }
    if let Some(cg) = &h.cgroup {
        let _ = fs::write(cg.join("cgroup.kill"), "1");
    }
    if let Some(pid) = h.pid.and_then(|p| rustix::process::Pid::from_raw(p as i32)) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
}

pub fn remove_cgroup(h: &Handle) {
    if let Some(cg) = &h.cgroup {
        let _ = fs::remove_dir(cg);
    }
}

pub fn exit_code(st: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(9))
}

pub fn far_future() -> tokio::time::Instant {
    tokio::time::Instant::now() + std::time::Duration::from_secs(86400 * 365)
}

pub async fn pump_output(
    mut src: impl tokio::io::AsyncRead + Unpin,
    rank: u32,
    err: bool,
    tx: tokio::sync::mpsc::Sender<(u32, bool, Vec<u8>)>,
) {
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 64 << 10];
    loop {
        match src.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.send((rank, err, buf[..n].to_vec())).await.is_err() {
                    break;
                }
            }
        }
    }
}
