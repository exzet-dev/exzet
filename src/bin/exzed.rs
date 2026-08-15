use anyhow::{bail, Context, Result};
use exzet::cas::{self, Store};
use exzet::cell;
use exzet::exfile::Task;
use exzet::proto::{self, JobInfo, Msg};
use exzet::rand_hex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};

struct JobEntry {
    name: String,
    world: u32,
    code: Option<i32>,
    kill: mpsc::Sender<()>,
}

struct Daemon {
    token: String,
    state: PathBuf,
    store: Store,
    containers: bool,
    tunnels: Mutex<HashMap<String, oneshot::Sender<TcpStream>>>,
    jobs: Mutex<HashMap<String, JobEntry>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut listen = format!("0.0.0.0:{}", exzet::DEFAULT_PORT);
    let mut state = default_state_dir();
    let mut token_arg: Option<String> = None;
    let mut containers = true;
    let mut svc: Option<bool> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--listen" => listen = args.next().context("--listen needs host:port")?,
            "--state" => state = PathBuf::from(args.next().context("--state needs a path")?),
            "--token" => token_arg = Some(args.next().context("--token needs a value")?),
            "--containers" => {
                containers = args
                    .next()
                    .context("--containers needs true or false")?
                    .parse()
                    .context("--containers needs true or false")?
            }
            "--service" => svc = Some(true),
            "--disable" => svc = Some(false),
            other => bail!(
                "unknown flag '{other}' (flags: --listen --state --token --containers --service --disable)"
            ),
        }
    }
    if let Some(enable) = svc {
        return service(enable, &listen, &state, token_arg, containers);
    }
    #[cfg(unix)]
    if containers && !exzet::is_root() {
        let root_sock = std::fs::metadata("/var/run/docker.sock")
            .is_ok_and(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0);
        if root_sock {
            bail!(
                "exzed is unprivileged but docker creates containers as root, which would outrank it; \
                 run exzed with sudo (at or above the container runtime's privilege level), \
                 or disable containers with --containers false"
            );
        }
    }
    std::fs::create_dir_all(&state).with_context(|| format!("creating {}", state.display()))?;
    for e in std::fs::read_dir(state.join("jobs")).into_iter().flatten().flatten() {
        unmount_work(&e.path().join("work"));
        let _ = std::fs::remove_dir_all(e.path());
    }
    for e in std::fs::read_dir(&state).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with("tmp-") { let _ = std::fs::remove_file(e.path()); }
    }
    let old = std::time::SystemTime::now() - Duration::from_secs(30 * 86400);
    for e in std::fs::read_dir(state.join("cas")).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
            if f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < old) { let _ = std::fs::remove_file(f.path()); }
        }
    }
    #[cfg(target_os = "linux")]
    for e in std::fs::read_dir(Path::new(cell::CG_ROOT).join("exzet")).into_iter().flatten().flatten() {
        let _ = std::fs::write(e.path().join("cgroup.kill"), "1");
        let _ = std::fs::remove_dir(e.path());
    }
    if let Ok(out) = Command::new("docker").args(["ps", "-aq", "-f", "name=^exzet-"]).output() {
        for id in String::from_utf8_lossy(&out.stdout).split_whitespace() {
            let _ = Command::new("docker").args(["rm", "-f", id]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
    let token = match token_arg {
        Some(t) => t,
        None => load_or_make_token(&state)?,
    };
    let store = Store::open(&state.join("cas"))?;
    let d = Arc::new(Daemon {
        token: token.clone(),
        state,
        store,
        containers,
        tunnels: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
    });
    let l = TcpListener::bind(&listen).await.with_context(|| format!("binding {listen}"))?;
    eprintln!("exzed listening on {listen}");
    eprintln!("server entry: {token}@{}:{}", host(), l.local_addr()?.port());
    #[cfg(target_os = "linux")]
    if !exzet::is_root() {
        eprintln!("exzed: not root, limits and live mode off");
    }
    #[cfg(target_os = "macos")]
    if !exzet::is_root() {
        eprintln!("exzed: not root, live mode off");
    }
    loop {
        let (conn, peer) = l.accept().await?;
        let d = d.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(d, conn).await {
                eprintln!("exzed: [{peer}] {e:#}");
            }
        });
    }
}

fn service(enable: bool, listen: &str, state: &Path, token: Option<String>, containers: bool) -> Result<()> {
    if enable {
        std::fs::create_dir_all(state)?;
        if let Some(t) = &token {
            std::fs::write(state.join("token"), t)?;
            exzet::set_mode(&state.join("token"), 0o600)?;
        }
    }
    install(enable, listen, state, containers)?;
    if enable {
        let token = load_or_make_token(state)?;
        eprintln!("server entry: {token}@{}:{}", host(), listen.rsplit(':').next().unwrap_or("7433"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install(enable: bool, listen: &str, state: &Path, containers: bool) -> Result<()> {
    let root = exzet::is_root();
    let run = |args: &[&str]| -> Result<bool> {
        let mut c = Command::new("systemctl");
        if !root {
            c.arg("--user");
        }
        Ok(c.args(args).status()?.success())
    };
    let unit = if root {
        PathBuf::from("/etc/systemd/system/exzed.service")
    } else {
        exzet::config_dir()
            .context("no config dir")?
            .join("systemd/user/exzed.service")
    };
    if !enable {
        if !run(&["disable", "--now", "exzed"])? || std::fs::remove_file(&unit).is_err() {
            bail!("no exzed service at this privilege level");
        }
        let _ = run(&["daemon-reload"]);
        return Ok(());
    }
    std::fs::create_dir_all(unit.parent().context("no unit dir")?)?;
    let wanted = if root { "multi-user.target" } else { "default.target" };
    std::fs::write(
        &unit,
        format!(
            "[Unit]\nDescription=exzet daemon\nAfter=network.target\n\n[Service]\nExecStart=\"{}\" --listen {listen} --state \"{}\" --containers {containers}\nRestart=always\n\n[Install]\nWantedBy={wanted}\n",
            std::env::current_exe()?.display(), state.display()
        ),
    )?;
    if !run(&["daemon-reload"])? || !run(&["enable", "exzed"])? || !run(&["restart", "exzed"])? {
        bail!("systemctl failed");
    }
    if !root && !Command::new("loginctl").arg("enable-linger").status()?.success() {
        eprintln!("exzed: enable-linger failed, service stops at logout");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn install(enable: bool, listen: &str, state: &Path, containers: bool) -> Result<()> {
    let root = exzet::is_root();
    let plist = if root {
        PathBuf::from("/Library/LaunchDaemons/net.exzet.exzed.plist")
    } else {
        PathBuf::from(std::env::var("HOME").context("no HOME")?).join("Library/LaunchAgents/net.exzet.exzed.plist")
    };
    let domain = if root { "system".to_string() } else { format!("gui/{}", rustix::process::getuid().as_raw()) };
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, &plist.to_string_lossy()])
        .stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !enable {
        std::fs::remove_file(&plist).context("no exzed service at this privilege level")?;
        return Ok(());
    }
    std::fs::create_dir_all(plist.parent().context("no plist dir")?)?;
    std::fs::write(
        &plist,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>net.exzet.exzed</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--listen</string><string>{listen}</string><string>--state</string><string>{}</string><string>--containers</string><string>{containers}</string></array>\n<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>\n</dict></plist>\n",
            std::env::current_exe()?.display(), state.display()
        ),
    )?;
    if !Command::new("launchctl").args(["bootstrap", &domain, &plist.to_string_lossy()]).status()?.success() {
        bail!("launchctl bootstrap failed");
    }
    Ok(())
}

#[cfg(windows)]
fn install(enable: bool, listen: &str, state: &Path, containers: bool) -> Result<()> {
    if !enable {
        let _ = Command::new("schtasks").args(["/End", "/TN", "exzed"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        if !Command::new("schtasks").args(["/Delete", "/F", "/TN", "exzed"]).status()?.success() {
            bail!("no exzed task");
        }
        return Ok(());
    }
    let tr = format!(
        "\"{}\" --listen {listen} --state \"{}\" --containers {containers}",
        std::env::current_exe()?.display(), state.display()
    );
    let admin = Command::new("schtasks")
        .args(["/Create", "/F", "/TN", "exzed", "/SC", "ONSTART", "/RU", "SYSTEM", "/TR", &tr])
        .status()?.success();
    if !admin
        && !Command::new("schtasks").args(["/Create", "/F", "/TN", "exzed", "/SC", "ONLOGON", "/TR", &tr]).status()?.success()
    {
        bail!("schtasks create failed");
    }
    let _ = Command::new("schtasks").args(["/Run", "/TN", "exzed"]).status();
    Ok(())
}

fn unmount_work(p: &Path) {
    #[cfg(target_os = "linux")]
    let _ = rustix::mount::unmount(p, rustix::mount::UnmountFlags::DETACH);
    #[cfg(target_os = "macos")]
    let _ = Command::new("umount").arg("-f").arg(p).stdout(Stdio::null()).stderr(Stdio::null()).status();
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        let _ = Command::new("umount").args(["-f", s.trim_end_matches('\\')]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

fn host() -> String {
    std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|c| c.split_whitespace().nth(2).map(str::to_string))
        .or_else(|| {
            let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
            s.connect("1.1.1.1:53").ok()?;
            Some(s.local_addr().ok()?.ip().to_string())
        })
        .unwrap_or_else(|| {
            #[cfg(unix)]
            return rustix::system::uname().nodename().to_string_lossy().into_owned();
            #[cfg(windows)]
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".into())
        })
}

fn default_state_dir() -> PathBuf {
    #[cfg(unix)]
    if exzet::is_root() {
        return PathBuf::from("/var/lib/exzet");
    }
    #[cfg(windows)]
    return std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("exzet");
    #[cfg(unix)]
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp")).join("exzet")
}

fn load_or_make_token(state: &Path) -> Result<String> {
    let tf = state.join("token");
    if tf.is_file() {
        return Ok(std::fs::read_to_string(&tf)?.trim().to_string());
    }
    let t = rand_hex(16);
    std::fs::write(&tf, &t)?;
    exzet::set_mode(&tf, 0o600)?;
    Ok(t)
}

async fn handle(d: Arc<Daemon>, conn: TcpStream) -> Result<()> {
    conn.set_nodelay(true)?;
    let (mut r, mut w) = conn.into_split();
    let msg = proto::recv_msg(&mut r).await?;
    let token = match &msg {
        Msg::Hello { token, .. } | Msg::TunnelJoin { token, .. } | Msg::Ps { token } | Msg::Attach { token, .. } => token,
        _ => bail!("unexpected first message"),
    };
    if !proto::token_eq(token, &d.token) {
        let _ = proto::send_msg(&mut w, &Msg::Err { msg: "bad token".into() }).await;
        drain(r).await;
        bail!("bad token");
    }
    match msg {
        Msg::TunnelJoin { tunnel, .. } => {
            let tx = d
                .tunnels
                .lock()
                .await
                .remove(&tunnel)
                .context("unknown tunnel id")?;
            let _ = tx.send(r.reunite(w)?);
            Ok(())
        }
        Msg::Ps { .. } => {
            let mut jobs: Vec<JobInfo> = d
                .jobs
                .lock()
                .await
                .iter()
                .map(|(k, e)| JobInfo { job: k.clone(), name: e.name.clone(), code: e.code })
                .collect();
            jobs.sort_by(|a, b| a.job.cmp(&b.job));
            proto::send_msg(&mut w, &Msg::Jobs { jobs }).await
        }
        Msg::Attach { job, .. } => attach_job(d, r, w, job).await,
        Msg::Hello { task, rank0, ranks, main, detach, .. } => {
            let name = task.name.clone();
            let code = run_job(&d, r, &mut w, task, rank0, ranks, main, detach).await?;
            if !detach {
                eprintln!("exzed: task '{name}' exited {code}");
            }
            Ok(())
        }
        _ => unreachable!(),
    }
}

// unread data RSTs the Err
async fn drain(mut r: OwnedReadHalf) {
    let mut buf = [0u8; 65536];
    while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(5), r.read(&mut buf)).await {
        if n == 0 {
            break;
        }
    }
}

async fn run_job(
    d: &Arc<Daemon>,
    r: OwnedReadHalf,
    w: &mut OwnedWriteHalf,
    task: Task,
    rank0: u32,
    ranks: u32,
    main: String,
    detach: bool,
) -> Result<i32> {
    let job = rand_hex(6);
    let jobdir = d.state.join("jobs").join(&job);
    let mut work = jobdir.join("work");

    let mut ro = Some(r);
    let res = prepare(d, &mut ro, w, &task, rank0, ranks, detach, &mut work).await;
    if let Err(e) = &res {
        let _ = proto::send_msg(w, &Msg::Err { msg: format!("{e:#}") }).await;
        if let Some(r) = ro.take() {
            drain(r).await;
        }
        unmount_work(&work);
        let _ = std::fs::remove_dir_all(&jobdir);
        return res.map(|_| 0);
    }

    let (ktx, mut krx) = mpsc::channel::<()>(1);
    let entry = JobEntry { name: task.name.clone(), world: task.replicas, code: None, kill: ktx.clone() };
    d.jobs.lock().await.insert(job.clone(), entry);
    if detach {
        proto::send_msg(w, &Msg::Started { job: job.clone() }).await?;
        let d = d.clone();
        tokio::spawn(async move {
            let kill = async {
                let _ = krx.recv().await;
            };
            let code = match tokio::fs::File::create(jobdir.join("log")).await {
                Ok(mut log) => run_task(&mut log, &task, rank0, ranks, &main, &job, &work, kill)
                    .await
                    .unwrap_or(1),
                Err(_) => 1,
            };
            if let Some(e) = d.jobs.lock().await.get_mut(&job) {
                e.code = Some(code);
            }
            let _ = std::fs::remove_dir_all(&work);
            eprintln!("exzed: job {job} ('{}') exited {code}", task.name);
        });
        return Ok(0);
    }

    let mut cr = ro.take().context("connection consumed")?;
    tokio::spawn(async move {
        loop {
            match proto::recv_msg(&mut cr).await {
                Ok(Msg::Kill) | Err(_) => {
                    let _ = ktx.send(()).await;
                    break;
                }
                Ok(_) => {}
            }
        }
    });
    let kill = async {
        let _ = krx.recv().await;
    };
    let res = run_task(w, &task, rank0, ranks, &main, &job, &work, kill).await;
    d.jobs.lock().await.remove(&job);
    if let Err(e) = &res {
        let _ = proto::send_msg(w, &Msg::Err { msg: format!("{e:#}") }).await;
    }
    unmount_work(&work);
    let _ = std::fs::remove_dir_all(&jobdir);
    res
}

async fn prepare(
    d: &Arc<Daemon>,
    ro: &mut Option<OwnedReadHalf>,
    w: &mut OwnedWriteHalf,
    task: &Task,
    rank0: u32,
    ranks: u32,
    detach: bool,
    work: &mut PathBuf,
) -> Result<()> {
    if ranks == 0 || rank0.checked_add(ranks).is_none_or(|e| e > task.replicas) {
        bail!("bad rank range");
    }
    if (task.cpus.is_some() || task.mem_bytes.is_some()) && task.image.is_none() && cell::limits_off() {
        proto::send_msg(w, &Msg::Warn { msg: "cpu/mem limits unenforced on this server".into() }).await?;
    }
    if task.live && task.replicas > 1 {
        bail!("live workspace requires replicas = 1");
    }
    if task.live && detach {
        bail!("detach requires the sync workspace");
    }
    std::fs::create_dir_all(&work)?;
    if let Some(image) = &task.image {
        if !d.containers {
            bail!("containers: false on this server");
        }
        if !cell::docker_ready().await {
            bail!("docker is not installed or not running on this server");
        }
        let mut insp = tokio::process::Command::new("docker");
        insp.args(["image", "inspect", image]).stdout(Stdio::null()).stderr(Stdio::null());
        let cached = insp.status().await?.success();
        if !cached {
            proto::send_msg(w, &Msg::Warn { msg: format!("pulling image {image}") }).await?;
            let out = tokio::process::Command::new("docker").args(["pull", image]).output().await?;
            if !out.status.success() {
                bail!(
                    "cannot access image '{image}': {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
    }
    if task.live {
        #[cfg(unix)]
        if !exzet::is_root() {
            bail!("live workspace requires exzed running as root");
        }
        let gate = TcpListener::bind(if cfg!(windows) { "127.0.0.1:2049" } else { "127.0.0.1:0" })
            .await
            .context(if cfg!(windows) { "binding 127.0.0.1:2049 (the Windows NFS client only mounts port 2049)" } else { "binding gate" })?;
        let port = gate.local_addr()?.port();
        let tun = rand_hex(12);
        let (tx, rx) = oneshot::channel();
        d.tunnels.lock().await.insert(tun.clone(), tx);
        proto::send_msg(w, &Msg::TunnelReady { tunnel: tun.clone() }).await?;
        let tconn = match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(c)) => c,
            _ => {
                d.tunnels.lock().await.remove(&tun);
                bail!("tunnel connection never arrived");
            }
        };
        tokio::spawn(proto::mux_acceptor(tconn, gate));
        #[cfg(target_os = "linux")]
        {
            let _ = tokio::process::Command::new("modprobe").arg("nfs").status().await;
            // kernel mounts; nolock skips NLM
            let data = std::ffi::CString::new(format!(
                "vers=3,proto=tcp,addr=127.0.0.1,port={port},mountvers=3,mountproto=tcp,mountport={port},nolock,soft,timeo=100,retrans=3,actimeo=1"
            ))?;
            let target = work.clone();
            let mountres = tokio::task::spawn_blocking(move || {
                rustix::mount::mount("127.0.0.1:/", &target, "nfs", rustix::mount::MountFlags::empty(), Some(data.as_c_str()))
            })
            .await;
            match mountres {
                Ok(Ok(())) => {}
                Ok(Err(e)) => bail!("nfs mount failed: {e}"),
                Err(e) => bail!("nfs mount failed: {e}"),
            }
        }
        #[cfg(target_os = "macos")]
        {
            let opts = format!("nolocks,vers=3,tcp,soft,port={port},mountport={port},actimeo=1");
            let st = tokio::process::Command::new("mount_nfs")
                .args(["-o", &opts, "127.0.0.1:/"]).arg(&*work)
                .status().await.context("running mount_nfs")?;
            if !st.success() {
                bail!("mount_nfs failed");
            }
        }
        #[cfg(windows)]
        {
            let _ = port;
            let drive = ('D'..='Z')
                .rev()
                .map(|l| format!("{l}:"))
                .find(|d| !Path::new(&format!("{d}\\")).exists())
                .context("no free drive letter")?;
            let st = tokio::process::Command::new("mount")
                .args(["-o", "anon,nolock", "127.0.0.1:/", &drive])
                .status().await
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => anyhow::anyhow!("mount not found - enable the 'client for nfs' windows feature"),
                    _ => e.into(),
                })?;
            if !st.success() {
                bail!("nfs mount of {drive} failed (is 'client for nfs' enabled?)");
            }
            *work = PathBuf::from(format!("{drive}\\"));
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        bail!("live workspace unsupported on this platform");
    } else {
        let r = ro.as_mut().context("connection consumed")?;
        let files = match proto::recv_msg(r).await? {
            Msg::Manifest { files } => files,
            _ => bail!("expected a manifest"),
        };
        for f in &files {
            proto::clean_rel(&f.path)?;
            if f.link.is_none() && !cas::valid_hash(&f.hash) {
                bail!("bad hash in manifest");
            }
        }
        let need = d.store.missing(&files);
        proto::send_msg(w, &Msg::Need { hashes: need.clone() }).await?;
        for _ in 0..need.len() {
            let (hash, len) = match proto::recv_msg(r).await? {
                Msg::Blob { hash, len } => (hash, len),
                _ => bail!("expected a blob"),
            };
            if !cas::valid_hash(&hash) {
                bail!("bad blob hash");
            }
            let tmp = d.state.join(format!("tmp-{}", rand_hex(6)));
            let mut tf = std::fs::File::create(&tmp)?;
            let res = proto::recv_chunks(r, len, |b| {
                use std::io::Write;
                tf.write_all(b)?;
                Ok(())
            })
            .await;
            drop(tf);
            if let Err(e) = res {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
            let d2 = d.clone();
            let h2 = hash.clone();
            tokio::task::spawn_blocking(move || d2.store.insert(&h2, &tmp)).await??;
        }
        let d2 = d.clone();
        let files2 = files;
        let work2 = work.clone();
        tokio::task::spawn_blocking(move || d2.store.hydrate(&files2, &work2)).await??;
    }
    if !detach {
        proto::send_msg(w, &Msg::Ready).await?;
    }
    Ok(())
}

async fn run_task<W: AsyncWrite + Unpin>(
    w: &mut W,
    task: &Task,
    rank0: u32,
    ranks: u32,
    main: &str,
    job: &str,
    work: &Path,
    kill: impl std::future::Future<Output = ()>,
) -> Result<i32> {
    let (otx, orx) = mpsc::channel(16);
    let j = cell::Job { task, id: job, dir: work, rank0, ranks, main, pipe: true };
    let (run, fwd) = tokio::join!(cell::run(j, otx, kill), forward(orx, w));
    fwd?;
    let (code, interrupted) = run?;
    if rank0 == 0 && !task.live && !interrupted && !task.outputs.is_empty() {
        let (outs, unmatched) = collect_outputs(work, &task.outputs)?;
        for spec in unmatched {
            proto::send_msg(w, &Msg::Warn { msg: format!("output '{spec}' matched nothing") }).await?;
        }
        for (rel, abs) in outs {
            let meta = abs.metadata()?;
            proto::send_msg(w, &Msg::OutFile {
                path: rel,
                mode: exzet::mode_of(&meta),
                len: meta.len(),
            })
            .await?;
            proto::send_file(w, &abs).await?;
        }
    }
    unmount_work(work);
    proto::send_msg(w, &Msg::Exit { code }).await?;
    Ok(code)
}

async fn forward<W: AsyncWrite + Unpin>(mut orx: mpsc::Receiver<cell::Chunk>, w: &mut W) -> Result<()> {
    let mut res = Ok(());
    while let Some((rank, err, data)) = orx.recv().await {
        if res.is_ok() {
            res = proto::send_msg(w, &Msg::Out { rank, err }).await;
        }
        if res.is_ok() {
            res = proto::send_raw(w, &data).await;
        }
    }
    res
}

async fn attach_job(d: Arc<Daemon>, mut r: OwnedReadHalf, mut w: OwnedWriteHalf, job: String) -> Result<()> {
    let world = match d.jobs.lock().await.get(&job) {
        Some(e) => e.world,
        None => {
            let _ = proto::send_msg(&mut w, &Msg::Err { msg: format!("unknown job '{job}'") }).await;
            bail!("unknown job '{job}'");
        }
    };
    let Ok(mut f) = tokio::fs::File::open(d.state.join("jobs").join(&job).join("log")).await else {
        let _ = proto::send_msg(&mut w, &Msg::Err { msg: format!("job '{job}' is not detached") }).await;
        bail!("job '{job}' has no log");
    };
    proto::send_msg(&mut w, &Msg::Attached { world }).await?;
    let (d2, job2) = (d.clone(), job.clone());
    tokio::spawn(async move {
        loop {
            match proto::recv_msg(&mut r).await {
                Ok(Msg::Kill) => {
                    if let Some(e) = d2.jobs.lock().await.get(&job2) {
                        let _ = e.kill.try_send(());
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let mut buf = vec![0u8; 64 << 10];
    let mut done = false;
    loop {
        let n = f.read(&mut buf).await?;
        if n > 0 {
            w.write_all(&buf[..n]).await?;
            continue;
        }
        if done {
            return Ok(());
        }
        done = d.jobs.lock().await.get(&job).is_none_or(|e| e.code.is_some());
        if !done {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn collect_outputs(work: &Path, specs: &[String]) -> Result<(Vec<(String, PathBuf)>, Vec<String>)> {
    let mut v = Vec::new();
    let mut unmatched = Vec::new();
    for spec in specs {
        let base = work.join(proto::clean_rel(spec)?);
        let before = v.len();
        collect_path(work, &base, &mut v)?;
        if v.len() == before {
            unmatched.push(spec.clone());
        }
    }
    v.sort();
    v.dedup();
    Ok((v, unmatched))
}

fn collect_path(work: &Path, p: &Path, v: &mut Vec<(String, PathBuf)>) -> Result<()> {
    if p.is_file() {
        v.push((p.strip_prefix(work)?.to_string_lossy().into_owned(), p.to_path_buf()));
    } else if p.is_dir() {
        for e in std::fs::read_dir(p)? {
            collect_path(work, &e?.path(), v)?;
        }
    }
    Ok(())
}
