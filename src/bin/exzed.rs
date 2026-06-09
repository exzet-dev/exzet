use anyhow::{bail, Context, Result};
use exzet::cas::{self, Store};
use exzet::cell::{self, Limits};
use exzet::exfile::Task;
use exzet::proto::{self, Msg};
use exzet::rand_hex;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};

struct Daemon {
    token: String,
    state: PathBuf,
    store: Store,
    containers: bool,
    tunnels: Mutex<HashMap<String, oneshot::Sender<TcpStream>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut listen = format!("0.0.0.0:{}", exzet::DEFAULT_PORT);
    let mut state = default_state_dir();
    let mut token_arg: Option<String> = None;
    let mut containers = true;
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
            "--service" => return service(true),
            "--disable" => return service(false),
            other => bail!(
                "unknown flag '{other}' (flags: --listen --state --token --containers --service --disable)"
            ),
        }
    }
    if containers && !rustix::process::geteuid().is_root() {
        let rootful_docker = tokio::process::Command::new("docker")
            .args(["info", "-f", "{{.SecurityOptions}}"])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| !String::from_utf8_lossy(&o.stdout).contains("rootless"));
        if rootful_docker == Some(true) {
            bail!(
                "exzed is unprivileged but docker creates containers as root, which would outrank it; \
                 run exzed with sudo (at or above the container runtime's privilege level), \
                 or disable containers with --containers false"
            );
        }
    }
    std::fs::create_dir_all(&state).with_context(|| format!("creating {}", state.display()))?;
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
    });
    let l = TcpListener::bind(&listen)
        .await
        .with_context(|| format!("binding {listen}"))?;
    eprintln!("exzed listening on {listen}");
    eprintln!("server entry: {token}@{}:{}", host(), l.local_addr()?.port());
    if !rustix::process::geteuid().is_root() {
        eprintln!("exzed: not root, limits and live mode off");
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

fn service(enable: bool) -> Result<()> {
    let root = rustix::process::geteuid().is_root();
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
        let _ = run(&["disable", "--now", "exzed"]);
        let _ = std::fs::remove_file(&unit);
        let _ = run(&["daemon-reload"]);
        return Ok(());
    }
    let state = default_state_dir();
    std::fs::create_dir_all(&state)?;
    let token = load_or_make_token(&state)?;
    std::fs::create_dir_all(unit.parent().context("no unit dir")?)?;
    let wanted = if root { "multi-user.target" } else { "default.target" };
    std::fs::write(
        &unit,
        format!(
            "[Unit]\nDescription=exzet daemon\n\n[Service]\nExecStart={}\nRestart=always\n\n[Install]\nWantedBy={wanted}\n",
            std::env::current_exe()?.display()
        ),
    )?;
    if !run(&["daemon-reload"])? || !run(&["enable", "--now", "exzed"])? {
        bail!("systemctl failed");
    }
    if !root {
        let _ = Command::new("loginctl").arg("enable-linger").status();
    }
    eprintln!("server entry: {token}@{}:{}", host(), exzet::DEFAULT_PORT);
    Ok(())
}

fn host() -> String {
    std::env::var("SSH_CONNECTION")
        .ok()
        .and_then(|c| c.split_whitespace().nth(2).map(str::to_string))
        .unwrap_or_else(|| rustix::system::uname().nodename().to_string_lossy().into_owned())
}

fn default_state_dir() -> PathBuf {
    if rustix::process::geteuid().is_root() {
        PathBuf::from("/var/lib/exzet")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("exzet")
    }
}

fn load_or_make_token(state: &Path) -> Result<String> {
    let tf = state.join("token");
    if tf.is_file() {
        return Ok(std::fs::read_to_string(&tf)?.trim().to_string());
    }
    let t = rand_hex(16);
    std::fs::write(&tf, &t)?;
    std::fs::set_permissions(&tf, std::fs::Permissions::from_mode(0o600))?;
    Ok(t)
}

async fn handle(d: Arc<Daemon>, conn: TcpStream) -> Result<()> {
    conn.set_nodelay(true)?;
    let (mut r, mut w) = conn.into_split();
    match proto::recv_msg(&mut r).await? {
        Msg::TunnelJoin { token, tunnel } => {
            if !proto::token_eq(&token, &d.token) {
                bail!("bad token");
            }
            let tx = d
                .tunnels
                .lock()
                .await
                .remove(&tunnel)
                .context("unknown tunnel id")?;
            let _ = tx.send(r.reunite(w)?);
            Ok(())
        }
        Msg::Hello { token, task } => {
            if !proto::token_eq(&token, &d.token) {
                let _ = proto::send_msg(&mut w, &Msg::Err { msg: "bad token".into() }).await;
                drain(r).await;
                bail!("bad token");
            }
            let name = task.name.clone();
            match run_job(&d, r, &mut w, task).await {
                Ok(code) => {
                    eprintln!("exzed: task '{name}' exited {code}");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        _ => bail!("unexpected first message"),
    }
}

// client may be mid-send when we error; read until it sees the Err and closes,
// else closing with unread data RSTs the connection and the message is lost
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
) -> Result<i32> {
    if task.live && task.replicas > 1 {
        bail!("live workspace requires replicas = 1");
    }
    let job = rand_hex(6);
    let jobdir = d.state.join("jobs").join(&job);
    let work = jobdir.join("work");
    std::fs::create_dir_all(&work)?;
    let mut mounted = false;

    let mut ro = Some(r);
    let res = job_inner(d, &mut ro, w, &task, &job, &work, &mut mounted).await;
    if let Err(e) = &res {
        let _ = proto::send_msg(w, &Msg::Err { msg: format!("{e:#}") }).await;
        if let Some(r) = ro.take() {
            drain(r).await;
        }
    }

    if mounted {
        let _ = rustix::mount::unmount(&work, rustix::mount::UnmountFlags::DETACH);
    }
    let _ = std::fs::remove_dir_all(&jobdir);
    res
}

async fn job_inner(
    d: &Arc<Daemon>,
    ro: &mut Option<OwnedReadHalf>,
    w: &mut OwnedWriteHalf,
    task: &Task,
    job: &str,
    work: &Path,
    mounted: &mut bool,
) -> Result<i32> {
    if let Some(image) = &task.image {
        if !d.containers {
            bail!("containers: false on this server");
        }
        let usable = tokio::process::Command::new("docker")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if !usable {
            bail!("docker is not installed or not running on this server");
        }
        let cached = tokio::process::Command::new("docker")
            .args(["image", "inspect", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?
            .success();
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
        if !rustix::process::geteuid().is_root() {
            bail!("live workspace requires exzed running as root");
        }
        let gate = TcpListener::bind("127.0.0.1:0").await?;
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
        let _ = tokio::process::Command::new("modprobe").arg("nfs").status().await;
        // kernel performs the MNT rpc itself; nolock skips NLM
        let data = std::ffi::CString::new(format!(
            "vers=3,proto=tcp,addr=127.0.0.1,port={port},mountvers=3,mountproto=tcp,mountport={port},nolock,soft,timeo=100,retrans=3,actimeo=1"
        ))?;
        let target = work.to_path_buf();
        let mountres = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                rustix::mount::mount(
                    "127.0.0.1:/",
                    &target,
                    "nfs",
                    rustix::mount::MountFlags::empty(),
                    Some(data.as_c_str()),
                )
            }),
        )
        .await;
        match mountres {
            Ok(Ok(Ok(()))) => *mounted = true,
            Ok(Ok(Err(e))) => bail!("nfs mount failed: {e}"),
            Ok(Err(e)) => bail!("nfs mount failed: {e}"),
            Err(_) => bail!("nfs mount timed out"),
        }
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
        let work2 = work.to_path_buf();
        tokio::task::spawn_blocking(move || d2.store.hydrate(&files2, &work2)).await??;
    }
    proto::send_msg(w, &Msg::Ready).await?;

    let lim = Limits { cpus: task.cpus, mem_bytes: task.mem_bytes };
    let (otx, mut orx) = mpsc::channel::<(u32, bool, Vec<u8>)>(16);
    let (wtx, mut wrx) = mpsc::channel::<std::io::Result<ExitStatus>>(task.replicas as usize);
    let mut running: Vec<cell::Handle> = Vec::new();
    for rank in 0..task.replicas {
        let mut c = match &task.image {
            Some(image) => cell::spawn_docker(
                job, rank, task.replicas, "127.0.0.1", &task.script, work, &lim, image, true,
            )?,
            None => {
                cell::spawn(job, rank, task.replicas, "127.0.0.1", &task.script, work, &lim, true)?
            }
        };
        let so = c.child.stdout.take().context("no stdout pipe")?;
        let se = c.child.stderr.take().context("no stderr pipe")?;
        tokio::spawn(cell::pump_output(so, rank, false, otx.clone()));
        tokio::spawn(cell::pump_output(se, rank, true, otx.clone()));
        running.push(c.handle.clone());
        let wtx = wtx.clone();
        let mut child = c.child;
        tokio::spawn(async move {
            let _ = wtx.send(child.wait().await).await;
        });
    }
    drop(otx);
    drop(wtx);

    let (ktx, mut krx) = oneshot::channel::<()>();
    let mut r = ro.take().context("connection consumed")?;
    tokio::spawn(async move {
        loop {
            match proto::recv_msg(&mut r).await {
                Ok(Msg::Kill) | Err(_) => {
                    let _ = ktx.send(());
                    break;
                }
                Ok(_) => {}
            }
        }
    });

    let deadline = task
        .time_secs
        .map(|s| tokio::time::Instant::now() + Duration::from_secs(s));
    let total = task.replicas as usize;
    let mut exited = 0usize;
    let mut code = 0i32;
    let mut pumps_open = true;
    let mut killed = false;
    let mut timed_out = false;
    let mut client_kill = false;
    while exited < total {
        tokio::select! {
            out = orx.recv(), if pumps_open => match out {
                Some((rank, err, data)) => {
                    proto::send_msg(w, &Msg::Out { rank, err }).await?;
                    proto::send_raw(w, &data).await?;
                }
                None => pumps_open = false,
            },
            st = wrx.recv() => if let Some(st) = st {
                exited += 1;
                let c = cell::exit_code(st?);
                if code == 0 {
                    code = c;
                }
            },
            _ = &mut krx, if !killed => {
                killed = true;
                client_kill = true;
                kill_all(&running);
            },
            _ = tokio::time::sleep_until(deadline.unwrap_or_else(cell::far_future)), if deadline.is_some() && !killed => {
                killed = true;
                timed_out = true;
                kill_all(&running);
            },
        }
    }
    while let Ok(Some((rank, err, data))) =
        tokio::time::timeout(Duration::from_millis(400), orx.recv()).await
    {
        proto::send_msg(w, &Msg::Out { rank, err }).await?;
        proto::send_raw(w, &data).await?;
    }
    if !task.live && !client_kill && !task.outputs.is_empty() {
        let (outs, unmatched) = collect_outputs(work, &task.outputs)?;
        for spec in unmatched {
            proto::send_msg(w, &Msg::Warn { msg: format!("output '{spec}' matched nothing") }).await?;
        }
        for (rel, abs) in outs {
            let meta = abs.metadata()?;
            proto::send_msg(w, &Msg::OutFile {
                path: rel,
                mode: meta.permissions().mode() & 0o777,
                len: meta.len(),
            })
            .await?;
            proto::send_file(w, &abs).await?;
        }
    }
    if timed_out {
        code = 124;
    }
    if client_kill {
        code = 130;
    }
    proto::send_msg(w, &Msg::Exit { code }).await?;
    for h in &running {
        cell::remove_cgroup(h);
    }
    Ok(code)
}

fn kill_all(running: &[cell::Handle]) {
    for h in running {
        cell::kill_tree(h);
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
        let rel = p.strip_prefix(work)?.to_string_lossy().into_owned();
        v.push((rel, p.to_path_buf()));
    } else if p.is_dir() {
        for e in std::fs::read_dir(p)? {
            collect_path(work, &e?.path(), v)?;
        }
    }
    Ok(())
}
