use anyhow::{bail, Context, Result};
use exzet::cas::{self, Scan};
use exzet::cell;
use exzet::exfile::{self, Task};
use exzet::nfs::WorkspaceFs;
use exzet::proto::{self, Frame, Msg};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

#[derive(Clone)]
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
    let mut detach = false;
    let mut cli_servers: Vec<String> = Vec::new();
    let mut exfile_arg: Option<String> = None;
    let mut pos: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--live" => live_flag = true,
            "--detach" | "-d" => detach = true,
            "--server" => cli_servers.push(args.next().context("--server needs [token@]host[:port]")?),
            "--exfile" | "-f" => exfile_arg = Some(args.next().context("-f needs a path")?),
            "-h" | "--help" => {
                println!("usage: exzec [--live] [--detach] [--server token@host:port]... [-f exfile] [task] [args]...");
                println!("       exzec ps | attach <job>");
                println!("no task: list tasks from the nearest exfile");
                return Ok(0);
            }
            f if f.starts_with('-') && pos.is_empty() => bail!("unknown flag '{f}'"),
            t => pos.push(t.to_string()),
        }
    }
    let found = match &exfile_arg {
        Some(p) => {
            let file = std::fs::canonicalize(p).with_context(|| p.clone())?;
            let root = file.parent().context("-f needs a file path")?.to_path_buf();
            Ok((root, file))
        }
        None => exfile::resolve(&std::env::current_dir()?),
    };
    if matches!(pos.first().map(String::as_str), Some("ps") | Some("attach")) {
        let (root, file_servers) = match &found {
            Ok((root, file)) => {
                let mut v: Vec<String> = Vec::new();
                let tasks = exfile::parse(&std::fs::read_to_string(file)?, &exfile::dotenv(root))?;
                for s in tasks.into_iter().flat_map(|t| t.servers) {
                    if !v.contains(&s) {
                        v.push(s);
                    }
                }
                (root.clone(), v)
            }
            Err(_) => (std::env::current_dir()?, Vec::new()),
        };
        let entries = pick_entries(&cli_servers, &file_servers);
        if entries.is_empty() {
            bail!("no servers configured");
        }
        let servers: Vec<Server> = entries.iter().map(|e| parse_entry(e)).collect();
        return match pos[0].as_str() {
            "attach" => attach(&servers, pos.get(1).context("attach needs a job id")?, &root).await,
            _ => ps(&servers).await,
        };
    }
    let (root, file) = found?;
    let text = std::fs::read_to_string(&file)?;
    let tasks = exfile::parse(&text, &exfile::dotenv(&root))?;
    let Some(name) = pos.first() else {
        for t in &tasks {
            let x = if t.replicas > 1 { format!(" x{}", t.replicas) } else { String::new() };
            let live = if t.live { " (live)" } else { "" };
            println!("{:<20} {}{}{}", t.name, t.script.lines().next().unwrap_or(""), x, live);
        }
        return Ok(0);
    };
    let chain = exfile::schedule(&tasks, name).with_context(|| format!("in {}", file.display()))?;
    let mut code = 0;
    for (i, t) in chain.iter().enumerate() {
        let last = i + 1 == chain.len();
        let mut task = (*t).clone();
        if last {
            task.live |= live_flag;
            task.args = pos[1..].to_vec();
        }
        if task.script.is_empty() {
            continue;
        }
        code = run_one(&root, task, &cli_servers, last && detach).await?;
        if code != 0 {
            break;
        }
    }
    Ok(code)
}

async fn run_one(root: &Path, task: Task, cli_servers: &[String], detach: bool) -> Result<i32> {
    if task.live && task.replicas > 1 {
        bail!("live workspace requires replicas = 1");
    }
    if task.live && detach {
        bail!("detach requires the sync workspace");
    }
    let entries = pick_entries(cli_servers, &task.servers);
    if entries.is_empty() {
        if detach {
            bail!("detach needs a server");
        }
        return run_local(root, &task).await;
    }
    let servers: Vec<Server> = entries.iter().map(|e| parse_entry(e)).collect();
    let want = (task.replicas as usize).min(servers.len());
    let mut picked: Vec<(TcpStream, Server)> = Vec::new();
    for s in &servers {
        if picked.len() == want {
            break;
        }
        match TcpStream::connect(&s.addr).await {
            Ok(c) => picked.push((c, s.clone())),
            Err(e) => eprintln!("exzec: {} unreachable: {e}", s.addr),
        }
    }
    if picked.is_empty() {
        bail!("no reachable server");
    }
    let scan = if task.live {
        None
    } else {
        let root = root.to_path_buf();
        Some(Arc::new(tokio::task::spawn_blocking(move || cas::scan(&root)).await??))
    };
    let k = picked.len() as u32;
    let main = match k {
        1 => "127.0.0.1".to_string(),
        _ => picked[0].1.addr.rsplit_once(':').map_or("127.0.0.1", |(h, _)| h.trim_matches(&['[', ']'][..])).to_string(),
    };
    let mut sessions = Vec::new();
    let mut rank0 = 0u32;
    for (i, (conn, srv)) in picked.into_iter().enumerate() {
        let ranks = task.replicas / k + u32::from((i as u32) < task.replicas % k);
        let (task, root, main, scan) = (task.clone(), root.to_path_buf(), main.clone(), scan.clone());
        sessions.push(tokio::spawn(session(conn, srv, root, task, rank0, ranks, main, scan, detach)));
        rank0 += ranks;
    }
    let mut code = 0;
    for s in sessions {
        let c = match s.await? {
            Ok(c) => c,
            Err(e) => {
                eprintln!("exzec: {e:#}");
                1
            }
        };
        if code == 0 {
            code = c;
        }
    }
    Ok(code)
}

async fn session(
    conn: TcpStream,
    srv: Server,
    root: PathBuf,
    task: Task,
    rank0: u32,
    ranks: u32,
    main: String,
    scan: Option<Arc<Scan>>,
    detach: bool,
) -> Result<i32> {
    conn.set_nodelay(true)?;
    let (mut r, mut w) = conn.into_split();
    let hello = Msg::Hello { token: srv.token.clone(), task: task.clone(), rank0, ranks, main, detach };
    proto::send_msg(&mut w, &hello).await?;

    if task.live {
        let tun = wait_for(&mut r, |m| if let Msg::TunnelReady { tunnel } = m { Some(tunnel) } else { None }).await?;
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
        let scan = scan.context("workspace scan missing")?;
        proto::send_msg(&mut w, &Msg::Manifest { files: scan.files.clone() }).await?;
        let need = wait_for(&mut r, |m| if let Msg::Need { hashes } = m { Some(hashes) } else { None }).await?;
        let n = need.len();
        for hash in need {
            let src = scan.by_hash.get(&hash).context("server requested an unknown blob")?;
            let len = src.metadata()?.len();
            proto::send_msg(&mut w, &Msg::Blob { hash, len }).await?;
            proto::send_file(&mut w, src).await?;
        }
        if n > 0 {
            eprintln!("exzec: uploaded {n} changed files to {}", srv.addr);
        }
    }
    if detach {
        let job = wait_for(&mut r, |m| if let Msg::Started { job } = m { Some(job) } else { None }).await?;
        eprintln!("exzec: job {job} on {}", srv.addr);
        return Ok(0);
    }
    wait_for(&mut r, |m| matches!(m, Msg::Ready).then_some(())).await?;
    if ranks == task.replicas {
        eprintln!("exzec: running '{}' on {}", task.name, srv.addr);
    } else {
        eprintln!("exzec: running '{}' ranks {rank0}-{} on {}", task.name, rank0 + ranks - 1, srv.addr);
    }
    watch_ctrlc(w);
    stream_job(&mut r, &root, task.replicas).await
}

async fn stream_job(r: &mut OwnedReadHalf, root: &Path, world: u32) -> Result<i32> {
    let mut lines: HashMap<(u32, bool), Vec<u8>> = HashMap::new();
    let mut pending_out: Option<(u32, bool)> = None;
    let mut pending_file: Option<(u64, u64, std::fs::File)> = None;
    loop {
        match proto::recv(r).await? {
            Frame::Json(m) => match m {
                Msg::Out { rank, err } => pending_out = Some((rank, err)),
                Msg::OutFile { path, mode, len } => {
                    let abs = root.join(proto::clean_rel(&path)?);
                    if let Some(p) = abs.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    let f = std::fs::File::create(&abs)?;
                    exzet::set_mode(&abs, (mode & 0o777) | 0o600)?;
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

async fn open(s: &Server, first: &Msg) -> Result<(OwnedReadHalf, OwnedWriteHalf)> {
    let conn = TcpStream::connect(&s.addr).await?;
    conn.set_nodelay(true)?;
    let (r, mut w) = conn.into_split();
    proto::send_msg(&mut w, first).await?;
    Ok((r, w))
}

async fn attach(servers: &[Server], job: &str, root: &Path) -> Result<i32> {
    for s in servers {
        let first = Msg::Attach { token: s.token.clone(), job: job.to_string() };
        let Ok((mut r, w)) = open(s, &first).await else {
            continue;
        };
        match proto::recv_msg(&mut r).await? {
            Msg::Attached { world } => {
                watch_ctrlc(w);
                return stream_job(&mut r, root, world).await;
            }
            Msg::Err { msg } => eprintln!("exzec: {}: {msg}", s.addr),
            _ => bail!("unexpected reply"),
        }
    }
    bail!("job '{job}' not found on any reachable server")
}

async fn ps(servers: &[Server]) -> Result<i32> {
    for s in servers {
        let (mut r, _w) = match open(s, &Msg::Ps { token: s.token.clone() }).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("exzec: {} unreachable: {e}", s.addr);
                continue;
            }
        };
        let jobs = wait_for(&mut r, |m| if let Msg::Jobs { jobs } = m { Some(jobs) } else { None }).await?;
        for j in jobs {
            let st = j.code.map_or("running".to_string(), |c| format!("exit {c}"));
            println!("{}  {:<20} {:<10} {}", j.job, j.name, st, s.addr);
        }
    }
    Ok(0)
}

async fn run_local(root: &Path, task: &Task) -> Result<i32> {
    if task.image.is_some() && !cell::docker_ready().await {
        bail!("docker is not installed or not running");
    }
    let job = exzet::rand_hex(6);
    let world = task.replicas;
    let (otx, mut orx) = tokio::sync::mpsc::channel::<cell::Chunk>(16);
    let j = cell::Job { task, id: &job, dir: root, rank0: 0, ranks: world, main: "127.0.0.1", pipe: world > 1 };
    let printer = async {
        let mut lines = HashMap::new();
        while let Some((rank, err, data)) = orx.recv().await {
            emit(world, rank, err, &data, &mut lines);
        }
        flush_lines(&mut lines);
    };
    let kill = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let (res, _) = tokio::join!(cell::run(j, otx, kill), printer);
    Ok(res?.0)
}

fn watch_ctrlc(mut w: OwnedWriteHalf) {
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nexzec: stopping remote job");
        let _ = proto::send_msg(&mut w, &Msg::Kill).await;
        let _ = tokio::signal::ctrl_c().await;
        std::process::exit(130);
    });
}

async fn wait_for<T>(r: &mut OwnedReadHalf, f: impl Fn(Msg) -> Option<T>) -> Result<T> {
    loop {
        match proto::recv_msg(r).await? {
            Msg::Warn { msg } => eprintln!("exzec: server: {msg}"),
            Msg::Err { msg } => bail!("{msg}"),
            m => return f(m).context("unexpected reply"),
        }
    }
}

fn pick_entries(cli: &[String], task: &[String]) -> Vec<String> {
    if !cli.is_empty() {
        cli.to_vec()
    } else if !task.is_empty() {
        task.to_vec()
    } else {
        config_entries()
    }
}

fn parse_entry(e: &str) -> Server {
    let (token, addr) = match e.split_once('@') {
        Some((t, a)) => (t.to_string(), a.to_string()),
        None => (String::new(), e.to_string()),
    };
    let ported = addr.rsplit_once(':').is_some_and(|(h, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (!h.contains(':') || h.ends_with(']')));
    let addr = if ported { addr } else { format!("{addr}:{}", exzet::DEFAULT_PORT) };
    Server { addr, token }
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
        return write_stream(err, data);
    }
    let buf = lines.entry((rank, err)).or_default();
    buf.extend_from_slice(data);
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = buf.drain(..=pos).collect();
        write_stream(err, &prefixed(rank, &line));
    }
    if buf.len() > proto::CHUNK {
        write_stream(err, &prefixed(rank, &std::mem::take(buf)));
    }
}

fn prefixed(rank: u32, data: &[u8]) -> Vec<u8> {
    let mut out = format!("[{rank}] ").into_bytes();
    out.extend_from_slice(data);
    out
}

fn flush_lines(lines: &mut HashMap<(u32, bool), Vec<u8>>) {
    for ((rank, err), mut buf) in lines.drain() {
        if !buf.is_empty() {
            buf.push(b'\n');
            write_stream(err, &prefixed(rank, &buf));
        }
    }
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

fn write_stream(err: bool, data: &[u8]) {
    let (mut o, mut e);
    let s: &mut dyn Write = if err { e = std::io::stderr().lock(); &mut e } else { o = std::io::stdout().lock(); &mut o };
    let _ = s.write_all(data).and_then(|()| s.flush());
}
