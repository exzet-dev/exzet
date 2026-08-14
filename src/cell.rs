use crate::exfile::Task;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub const CG_ROOT: &str = "/sys/fs/cgroup";

pub type Chunk = (u32, bool, Vec<u8>);

pub struct Job<'a> {
    pub task: &'a Task,
    pub id: &'a str,
    pub dir: &'a Path,
    pub rank0: u32,
    pub ranks: u32,
    pub main: &'a str,
    pub pipe: bool,
}

#[derive(Clone)]
struct Handle {
    pid: Option<u32>,
    cgroup: Option<PathBuf>,
    container: Option<String>,
}

pub async fn docker_ready() -> bool {
    let ver = Command::new("docker").arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status();
    ver.await.map(|s| s.success()).unwrap_or(false)
}

pub fn limits_off() -> bool {
    !rustix::process::geteuid().is_root()
        || !Path::new(CG_ROOT).join("cgroup.controllers").is_file()
}

fn make_cgroup(name: &str, task: &Task) -> Option<PathBuf> {
    if limits_off() {
        return None;
    }
    let base = Path::new(CG_ROOT).join("exzet");
    fs::create_dir_all(&base).ok()?;
    let _ = fs::write(Path::new(CG_ROOT).join("cgroup.subtree_control"), "+cpu +memory");
    let _ = fs::write(base.join("cgroup.subtree_control"), "+cpu +memory");
    let cg = base.join(name);
    fs::create_dir_all(&cg).ok()?;
    if let Some(c) = task.cpus {
        let _ = fs::write(cg.join("cpu.max"), format!("{} 100000", c as u64 * 100_000));
    }
    if let Some(m) = task.mem_bytes {
        let _ = fs::write(cg.join("memory.max"), m.to_string());
    }
    Some(cg)
}

fn spawn(j: &Job, rank: u32) -> Result<(Child, Handle)> {
    let envs = [
        ("EXZET_JOB", j.id.to_string()),
        ("EXZET_RANK", rank.to_string()),
        ("EXZET_WORLD", j.task.replicas.to_string()),
        ("EXZET_MAIN", j.main.to_string()),
    ];
    let (mut cmd, mut handle) = match &j.task.image {
        Some(image) => {
            let name = format!("exzet-{}-{rank}", j.id);
            let mut cmd = Command::new("docker");
            cmd.args(["run", "--rm", "--network", "host", "--name", &name]).arg("-v")
                .arg(format!("{}:/work", j.dir.display())).args(["-w", "/work", "--entrypoint", "/bin/sh"]);
            for (k, v) in &envs {
                cmd.arg("-e").arg(format!("{k}={v}"));
            }
            if let Some(c) = j.task.cpus {
                cmd.arg("--cpus").arg(c.to_string());
            }
            if let Some(m) = j.task.mem_bytes {
                cmd.arg("--memory").arg(m.to_string());
            }
            if j.task.gpus > 0 {
                cmd.arg("--gpus").arg(j.task.gpus.to_string());
            }
            cmd.arg(image).args(["-euc", &j.task.script]);
            (cmd, Handle { pid: None, cgroup: None, container: Some(name) })
        }
        None => {
            let cgroup = make_cgroup(&format!("{}-{rank}", j.id), j.task);
            let script = match &cgroup {
                Some(cg) => format!("echo $$ >{}/cgroup.procs\n{}", cg.display(), j.task.script),
                None => j.task.script.clone(),
            };
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-euc").arg(script).current_dir(j.dir).envs(envs);
            (cmd, Handle { pid: None, cgroup, container: None })
        }
    };
    cmd.stdin(Stdio::null()).process_group(0).kill_on_drop(true);
    if j.pipe {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let child = cmd.spawn().context("spawning cell")?;
    handle.pid = child.id();
    Ok((child, handle))
}

pub async fn run(
    j: Job<'_>,
    out: mpsc::Sender<Chunk>,
    kill: impl std::future::Future<Output = ()>,
) -> Result<(i32, bool)> {
    let (wtx, mut wrx) = mpsc::channel(j.ranks.max(1) as usize);
    let mut handles = Vec::new();
    let mut pumps = Vec::new();
    for rank in j.rank0..j.rank0 + j.ranks {
        let (mut child, handle) = spawn(&j, rank)?;
        if j.pipe {
            let so = child.stdout.take().context("no stdout pipe")?;
            let se = child.stderr.take().context("no stderr pipe")?;
            pumps.push(tokio::spawn(pump(so, rank, false, out.clone())));
            pumps.push(tokio::spawn(pump(se, rank, true, out.clone())));
        }
        handles.push(handle);
        let wtx = wtx.clone();
        tokio::spawn(async move {
            let _ = wtx.send(child.wait().await).await;
        });
    }
    drop(wtx);
    drop(out);
    let deadline = j
        .task
        .time_secs
        .map(|s| tokio::time::Instant::now() + Duration::from_secs(s));
    let far = tokio::time::Instant::now() + Duration::from_secs(86400 * 365);
    tokio::pin!(kill);
    let (mut exited, mut code) = (0u32, 0i32);
    let (mut killed, mut interrupted, mut timed_out) = (false, false, false);
    while exited < j.ranks {
        tokio::select! {
            st = wrx.recv() => if let Some(st) = st {
                exited += 1;
                let c = exit_code(st?);
                if code == 0 {
                    code = c;
                }
            },
            _ = &mut kill, if !killed => {
                killed = true;
                interrupted = true;
                for h in &handles {
                    kill_tree(h);
                }
            },
            _ = tokio::time::sleep_until(deadline.unwrap_or(far)), if deadline.is_some() && !killed => {
                killed = true;
                timed_out = true;
                for h in &handles {
                    kill_tree(h);
                }
            },
        }
    }
    let stop = tokio::time::Instant::now() + Duration::from_millis(400);
    for mut p in pumps {
        if tokio::time::timeout_at(stop, &mut p).await.is_err() {
            p.abort();
        }
    }
    for cg in handles.iter().filter_map(|h| h.cgroup.as_ref()) {
        let _ = fs::write(cg.join("cgroup.kill"), "1");
        let _ = fs::remove_dir(cg);
    }
    if timed_out {
        code = 124;
    }
    if interrupted {
        code = 130;
    }
    Ok((code, interrupted))
}

fn kill_tree(h: &Handle) {
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

fn exit_code(st: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(9))
}

async fn pump(
    mut src: impl tokio::io::AsyncRead + Unpin,
    rank: u32,
    err: bool,
    tx: mpsc::Sender<Chunk>,
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
