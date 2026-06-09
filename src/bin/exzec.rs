use anyhow::{bail, Context, Result};
use exzet::exfile;
use exzet::nfs::WorkspaceFs;
use exzet::proto::{self, Frame, Msg};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use tokio::net::TcpStream;

struct Server {
    addr: String,
    token: String,
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("exzec: {e:#}");
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<i32> {
    let mut live_flag = false;
    let mut cli_servers: Vec<String> = Vec::new();
    let mut exfile_arg: Option<String> = None;
    let mut name: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--live" => live_flag = true,
            "--server" => cli_servers.push(args.next().context("--server needs [token@]host[:port]")?),
            "--exfile" | "-f" => exfile_arg = Some(args.next().context("-f needs a path")?),
            "-h" | "--help" => {
                println!("usage: exzec [--live] [--server token@host:port]... [-f exfile] [task]");
                println!("no task: list tasks from the nearest exfile");
                return Ok(0);
            }
            f if f.starts_with('-') => bail!("unknown flag '{f}'"),
            t => {
                if name.is_some() {
                    bail!("one task at a time");
                }
                name = Some(t.to_string());
            }
        }
    }
    let (root, file) = match &exfile_arg {
        Some(p) => {
            let file = std::fs::canonicalize(p).with_context(|| p.clone())?;
            let root = file.parent().context("-f needs a file path")?.to_path_buf();
            (root, file)
        }
        None => exfile::resolve(&std::env::current_dir()?)?,
    };
    let text = std::fs::read_to_string(&file)?;
    let tasks = exfile::parse(&text)?;
    let Some(name) = name else {
        for t in &tasks {
            let mut marks = String::new();
            if t.replicas > 1 {
                marks.push_str(&format!(" x{}", t.replicas));
            }
            if t.live {
                marks.push_str(" (live)");
            }
            println!("{:<20} {}{}", t.name, t.script.lines().next().unwrap_or(""), marks);
        }
        return Ok(0);
    };
    let mut task = tasks
        .into_iter()
        .find(|t| t.name == name)
        .with_context(|| format!("no task '{name}' in {}", file.display()))?;
    task.live |= live_flag;
    if task.live && task.replicas > 1 {
        bail!("live workspace requires replicas = 1");
    }

    let entries = if !cli_servers.is_empty() {
        cli_servers
    } else if !task.servers.is_empty() {
        task.servers.clone()
    } else {
        config_entries()
    };
    if entries.is_empty() {
        return run_local(&root, &task).await;
    }
    let servers: Vec<Server> = entries.iter().map(|e| parse_entry(e)).collect();

    let mut picked = None;
    for s in &servers {
        match TcpStream::connect(&s.addr).await {
            Ok(c) => {
                picked = Some((c, s));
                break;
            }
            Err(e) => eprintln!("exzec: {} unreachable: {e}", s.addr),
        }
    }
    let (conn, srv) = picked.context("no reachable server")?;
    conn.set_nodelay(true)?;
    let (mut r, mut w) = conn.into_split();
    proto::send_msg(&mut w, &Msg::Hello { token: srv.token.clone(), task: task.clone() }).await?;

    if task.live {
        let tun = loop {
            match proto::recv_msg(&mut r).await? {
                Msg::TunnelReady { tunnel } => break tunnel,
                Msg::Warn { msg } => eprintln!("exzec: server: {msg}"),
                Msg::Err { msg } => bail!("{msg}"),
                _ => bail!("unexpected reply"),
            }
        };
        let nfsl = NFSTcpListener::bind("127.0.0.1:0", WorkspaceFs::new(root.clone())).await?;
        let nfs_port = nfsl.get_listen_port();
        tokio::spawn(async move {
            let _ = nfsl.handle_forever().await;
        });
        let mut t = TcpStream::connect(&srv.addr).await?;
        t.set_nodelay(true)?;
        proto::send_msg(&mut t, &Msg::TunnelJoin { token: srv.token.clone(), tunnel: tun }).await?;
        let target: SocketAddr = format!("127.0.0.1:{nfs_port}").parse()?;
        tokio::spawn(proto::mux_dialer(t, target));
    } else {
        let scan = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || exzet::cas::scan(&root)).await??
        };
        proto::send_msg(&mut w, &Msg::Manifest { files: scan.files }).await?;
        let need = loop {
            match proto::recv_msg(&mut r).await? {
                Msg::Need { hashes } => break hashes,
                Msg::Warn { msg } => eprintln!("exzec: server: {msg}"),
                Msg::Err { msg } => bail!("{msg}"),
                _ => bail!("unexpected reply"),
            }
        };
        let n = need.len();
        for hash in need {
            let src = scan.by_hash.get(&hash).context("server requested an unknown blob")?;
            let len = src.metadata()?.len();
            proto::send_msg(&mut w, &Msg::Blob { hash, len }).await?;
            proto::send_file(&mut w, src).await?;
        }
        if n > 0 {
            eprintln!("exzec: uploaded {n} changed files");
        }
    }

    loop {
        match proto::recv_msg(&mut r).await? {
            Msg::Ready => break,
            Msg::Warn { msg } => eprintln!("exzec: server: {msg}"),
            Msg::Err { msg } => bail!("{msg}"),
            _ => bail!("unexpected reply"),
        }
    }
    eprintln!("exzec: running '{}' on {}", task.name, srv.addr);

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nexzec: stopping remote job");
        let _ = proto::send_msg(&mut w, &Msg::Kill).await;
        let _ = tokio::signal::ctrl_c().await;
        std::process::exit(130);
    });

    let world = task.replicas;
    let mut lines: HashMap<(u32, bool), Vec<u8>> = HashMap::new();
    let mut pending_out: Option<(u32, bool)> = None;
    let mut pending_file: Option<(u64, u64, std::fs::File)> = None;
    loop {
        match proto::recv(&mut r).await? {
            Frame::Json(m) => match m {
                Msg::Out { rank, err } => pending_out = Some((rank, err)),
                Msg::OutFile { path, mode, len } => {
                    let abs = root.join(proto::clean_rel(&path)?);
                    if let Some(p) = abs.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    let f = std::fs::File::create(&abs)?;
                    f.set_permissions(std::fs::Permissions::from_mode((mode & 0o777) | 0o600))?;
                    eprintln!("exzec: output {path}");
                    if len > 0 {
                        pending_file = Some((len, 0, f));
                    }
                }
                Msg::Exit { code } => {
                    flush_lines(&mut lines);
                    return Ok(code);
                }
                Msg::Warn { msg } => eprintln!("exzec: server: {msg}"),
                Msg::Err { msg } => bail!("{msg}"),
                _ => {}
            },
            Frame::Raw(data) => {
                if let Some((rank, err)) = pending_out.take() {
                    emit(world, rank, err, &data, &mut lines);
                } else if let Some((len, mut got, mut f)) = pending_file.take() {
                    f.write_all(&data)?;
                    got += data.len() as u64;
                    if got < len {
                        pending_file = Some((len, got, f));
                    }
                }
            }
        }
    }
}

async fn run_local(root: &std::path::Path, task: &exfile::Task) -> Result<i32> {
    use exzet::cell::{self, Limits};
    if task.image.is_some() {
        let usable = tokio::process::Command::new("docker")
            .arg("version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if !usable {
            bail!("docker is not installed or not running");
        }
    }
    let job = exzet::rand_hex(6);
    let lim = Limits { cpus: task.cpus, mem_bytes: task.mem_bytes };
    let world = task.replicas;
    let pipe = world > 1;
    let (otx, mut orx) = tokio::sync::mpsc::channel::<(u32, bool, Vec<u8>)>(16);
    let (wtx, mut wrx) =
        tokio::sync::mpsc::channel::<std::io::Result<std::process::ExitStatus>>(world as usize);
    let mut handles = Vec::new();
    for rank in 0..world {
        let mut c = match &task.image {
            Some(image) => cell::spawn_docker(
                &job, rank, world, "127.0.0.1", &task.script, root, &lim, image, pipe,
            )?,
            None => cell::spawn(&job, rank, world, "127.0.0.1", &task.script, root, &lim, pipe)?,
        };
        if pipe {
            let so = c.child.stdout.take().context("no stdout pipe")?;
            let se = c.child.stderr.take().context("no stderr pipe")?;
            tokio::spawn(cell::pump_output(so, rank, false, otx.clone()));
            tokio::spawn(cell::pump_output(se, rank, true, otx.clone()));
        }
        handles.push(c.handle.clone());
        let wtx = wtx.clone();
        let mut child = c.child;
        tokio::spawn(async move {
            let _ = wtx.send(child.wait().await).await;
        });
    }
    drop(otx);
    drop(wtx);

    let deadline = task
        .time_secs
        .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs(s));
    let mut lines: HashMap<(u32, bool), Vec<u8>> = HashMap::new();
    let total = world as usize;
    let mut exited = 0usize;
    let mut code = 0i32;
    let mut pumps_open = pipe;
    let mut killed = false;
    let mut timed_out = false;
    let mut interrupted = false;
    while exited < total {
        tokio::select! {
            out = orx.recv(), if pumps_open => match out {
                Some((rank, err, data)) => emit(world, rank, err, &data, &mut lines),
                None => pumps_open = false,
            },
            st = wrx.recv() => if let Some(st) = st {
                exited += 1;
                let c = cell::exit_code(st?);
                if code == 0 {
                    code = c;
                }
            },
            _ = tokio::signal::ctrl_c(), if !killed => {
                killed = true;
                interrupted = true;
                for h in &handles {
                    cell::kill_tree(h);
                }
            },
            _ = tokio::time::sleep_until(deadline.unwrap_or_else(cell::far_future)), if deadline.is_some() && !killed => {
                killed = true;
                timed_out = true;
                for h in &handles {
                    cell::kill_tree(h);
                }
            },
        }
    }
    while let Ok(Some((rank, err, data))) =
        tokio::time::timeout(std::time::Duration::from_millis(400), orx.recv()).await
    {
        emit(world, rank, err, &data, &mut lines);
    }
    flush_lines(&mut lines);
    for h in &handles {
        cell::remove_cgroup(h);
    }
    if timed_out {
        code = 124;
    }
    if interrupted {
        code = 130;
    }
    Ok(code)
}

fn norm_addr(a: String) -> String {
    if a.contains(':') {
        a
    } else {
        format!("{a}:{}", exzet::DEFAULT_PORT)
    }
}

fn parse_entry(e: &str) -> Server {
    let (token, addr) = match e.split_once('@') {
        Some((t, a)) => (t.to_string(), a.to_string()),
        None => (String::new(), e.to_string()),
    };
    Server { addr: norm_addr(addr), token }
}

fn config_entries() -> Vec<String> {
    let Some(base) = exzet::config_dir() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(base.join("exzet/servers")) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn emit(world: u32, rank: u32, err: bool, data: &[u8], lines: &mut HashMap<(u32, bool), Vec<u8>>) {
    if world <= 1 {
        write_stream(err, data);
        return;
    }
    let buf = lines.entry((rank, err)).or_default();
    buf.extend_from_slice(data);
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buf.drain(..=pos).collect();
        let mut out = format!("[{rank}] ").into_bytes();
        out.extend_from_slice(&line);
        write_stream(err, &out);
    }
}

fn flush_lines(lines: &mut HashMap<(u32, bool), Vec<u8>>) {
    for ((rank, err), mut buf) in lines.drain() {
        if buf.is_empty() {
            continue;
        }
        buf.push(b'\n');
        let mut out = format!("[{rank}] ").into_bytes();
        out.extend_from_slice(&buf);
        write_stream(err, &out);
    }
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

fn write_stream(err: bool, data: &[u8]) {
    if err {
        let mut se = std::io::stderr().lock();
        let _ = se.write_all(data);
        let _ = se.flush();
    } else {
        let mut so = std::io::stdout().lock();
        let _ = so.write_all(data);
        let _ = so.flush();
    }
}
