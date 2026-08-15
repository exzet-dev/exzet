use crate::exfile::Task;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[cfg(target_os = "linux")]
pub const CG_ROOT: &str = "/sys/fs/cgroup";
const CELL: &str = "/.exzet-cell";

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

#[derive(Default)]
struct Handle {
    pid: Option<u32>,
    container: Option<String>,
    file: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    cgroup: Option<PathBuf>,
    #[cfg(windows)]
    job: Option<isize>,
}

pub async fn docker_ready() -> bool {
    let ver = Command::new("docker").arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status();
    ver.await.map(|s| s.success()).unwrap_or(false)
}

pub fn limits_off() -> bool {
    #[cfg(target_os = "linux")]
    return !crate::is_root() || !Path::new(CG_ROOT).join("cgroup.controllers").is_file();
    #[cfg(all(unix, not(target_os = "linux")))]
    return true;
    #[cfg(windows)]
    false
}

#[cfg(target_os = "linux")]
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

fn script_file(t: &Task, id: &str, rank: u32, ext: &str) -> Result<PathBuf> {
    let p = std::env::temp_dir().join(format!("exzet-{id}-{rank}{ext}"));
    fs::write(&p, format!("{}\n", t.script))?;
    crate::set_mode(&p, 0o755)?;
    Ok(p)
}

fn interp(line: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for t in line.trim_start_matches("#!").split_whitespace() {
        let base = t.rsplit('/').next().unwrap_or(t);
        if v.is_empty() && base == "env" {
            continue;
        }
        v.push(if v.is_empty() { base.to_string() } else { t.to_string() });
    }
    if v.is_empty() {
        v.push("sh".to_string());
    }
    v
}

fn invoke(t: &Task, id: &str, rank: u32) -> Result<(Vec<String>, Option<PathBuf>)> {
    let docker = t.image.is_some();
    let mut file = None;
    let mut argv: Vec<String>;
    if t.script.starts_with("#!") {
        let f = script_file(t, id, rank, "")?;
        argv = if docker {
            vec![CELL.to_string()]
        } else if cfg!(windows) {
            let mut v = interp(t.script.lines().next().unwrap_or(""));
            v.push(f.display().to_string());
            v
        } else {
            vec![f.display().to_string()]
        };
        file = Some(f);
    } else if !t.shell.is_empty() {
        argv = t.shell.split_whitespace().map(str::to_string).collect();
        argv.push(t.script.clone());
    } else if docker || cfg!(unix) {
        argv = vec!["/bin/sh".into(), "-euc".into(), t.script.clone(), t.name.clone()];
    } else {
        let f = script_file(t, id, rank, ".ps1")?;
        argv = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"].map(String::from).into();
        argv.push(f.display().to_string());
        file = Some(f);
    }
    argv.extend(t.args.iter().cloned());
    Ok((argv, file))
}

fn spawn(j: &Job, rank: u32) -> Result<(Child, Handle)> {
    let envs = [
        ("EXZET_JOB", j.id.to_string()),
        ("EXZET_RANK", rank.to_string()),
        ("EXZET_WORLD", j.task.replicas.to_string()),
        ("EXZET_MAIN", j.main.to_string()),
    ];
    let (argv, file) = invoke(j.task, j.id, rank)?;
    let mut handle = Handle::default();
    handle.file = file;
    let mut cmd = match &j.task.image {
        Some(image) => {
            let name = format!("exzet-{}-{rank}", j.id);
            let mut cmd = Command::new("docker");
            cmd.args(["run", "--rm", "--network", "host", "--name", &name]).arg("-v")
                .arg(format!("{}:/work", j.dir.display())).args(["-w", "/work", "--entrypoint", &argv[0]]);
            if let Some(f) = &handle.file {
                cmd.arg("-v").arg(format!("{}:{CELL}:ro", f.display()));
            }
            for (k, v) in j.task.env.iter().map(|(k, v)| (k.as_str(), v)).chain(envs.iter().map(|(k, v)| (*k, v))) {
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
            cmd.arg(image).args(&argv[1..]);
            handle.container = Some(name);
            cmd
        }
        None => {
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..]).current_dir(j.dir);
            for (k, v) in &j.task.env {
                cmd.env(k, v);
            }
            cmd.envs(envs);
            #[cfg(target_os = "linux")]
            {
                handle.cgroup = make_cgroup(&format!("{}-{rank}", j.id), j.task);
                if let Some(cg) = &handle.cgroup {
                    use std::os::unix::ffi::OsStringExt;
                    let procs = std::ffi::CString::new(cg.join("cgroup.procs").into_os_string().into_vec())?;
                    unsafe {
                        cmd.pre_exec(move || {
                            let fd = rustix::fs::open(procs.as_c_str(), rustix::fs::OFlags::WRONLY, rustix::fs::Mode::empty())?;
                            rustix::io::write(&fd, b"0")?;
                            Ok(())
                        });
                    }
                }
            }
            cmd
        }
    };
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    if j.pipe {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let child = cmd.spawn().context("spawning cell")?;
    handle.pid = child.id();
    #[cfg(windows)]
    if handle.container.is_none() {
        handle.job = assign_job(&child, j.task);
    }
    Ok((child, handle))
}

#[cfg(windows)]
fn assign_job(child: &Child, t: &Task) -> Option<isize> {
    use windows_sys::Win32::System::JobObjects::*;
    let ph = child.raw_handle()?;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return None;
        }
        let mut ext: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        ext.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Some(m) = t.mem_bytes {
            ext.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            ext.JobMemoryLimit = m as usize;
        }
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const ext).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if let Some(c) = t.cpus {
            let total = std::thread::available_parallelism().map_or(1, |n| n.get()) as u32;
            let mut rate: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = std::mem::zeroed();
            rate.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            rate.Anonymous.CpuRate = (c.min(total) * 10000 / total).max(1);
            SetInformationJobObject(
                job,
                JobObjectCpuRateControlInformation,
                (&raw const rate).cast(),
                size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            );
        }
        AssignProcessToJobObject(job, ph);
        Some(job as isize)
    }
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
    for h in &handles {
        #[cfg(target_os = "linux")]
        if let Some(cg) = &h.cgroup {
            let _ = fs::write(cg.join("cgroup.kill"), "1");
            let _ = fs::remove_dir(cg);
        }
        #[cfg(windows)]
        if let Some(job) = h.job {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job as _) };
        }
        if let Some(f) = &h.file {
            let _ = fs::remove_file(f);
        }
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
    #[cfg(target_os = "linux")]
    if let Some(cg) = &h.cgroup {
        let _ = fs::write(cg.join("cgroup.kill"), "1");
    }
    #[cfg(unix)]
    if let Some(pid) = h.pid.and_then(|p| rustix::process::Pid::from_raw(p as i32)) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    if let Some(job) = h.job {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(job as _, 1) };
    }
}

fn exit_code(st: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(9))
    }
    #[cfg(windows)]
    st.code().unwrap_or(1)
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
