use crate::exfile::Task;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

pub const CHUNK: usize = 1 << 20;
const MAX_FRAME: usize = CHUNK + 64;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub mode: u32,
    pub hash: String,
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Msg {
    Hello { token: String, task: Task },
    TunnelJoin { token: String, tunnel: String },
    Manifest { files: Vec<FileEntry> },
    Need { hashes: Vec<String> },
    Blob { hash: String, len: u64 },
    TunnelReady { tunnel: String },
    Ready,
    Out { rank: u32, err: bool },
    OutFile { path: String, mode: u32, len: u64 },
    Exit { code: i32 },
    Kill,
    Warn { msg: String },
    Err { msg: String },
}

pub enum Frame {
    Json(Msg),
    Raw(Vec<u8>),
}

pub async fn send_msg<W: AsyncWrite + Unpin>(w: &mut W, msg: &Msg) -> Result<()> {
    let body = serde_json::to_vec(msg)?;
    w.write_u32((body.len() + 1) as u32).await?;
    w.write_u8(b'J').await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

pub async fn send_raw<W: AsyncWrite + Unpin>(w: &mut W, bytes: &[u8]) -> Result<()> {
    w.write_u32((bytes.len() + 1) as u32).await?;
    w.write_u8(b'R').await?;
    w.write_all(bytes).await?;
    w.flush().await?;
    Ok(())
}

pub async fn recv<R: AsyncRead + Unpin>(r: &mut R) -> Result<Frame> {
    let len = r.read_u32().await.context("connection closed")? as usize;
    if len == 0 || len > MAX_FRAME {
        bail!("bad frame length {len}");
    }
    let tag = r.read_u8().await?;
    let mut buf = vec![0u8; len - 1];
    r.read_exact(&mut buf).await?;
    match tag {
        b'J' => Ok(Frame::Json(serde_json::from_slice(&buf)?)),
        b'R' => Ok(Frame::Raw(buf)),
        t => bail!("bad frame tag {t}"),
    }
}

pub async fn recv_msg<R: AsyncRead + Unpin>(r: &mut R) -> Result<Msg> {
    match recv(r).await? {
        Frame::Json(m) => Ok(m),
        Frame::Raw(_) => bail!("expected a control message"),
    }
}

pub async fn recv_chunks<R, F>(r: &mut R, len: u64, mut sink: F) -> Result<()>
where
    R: AsyncRead + Unpin,
    F: FnMut(&[u8]) -> Result<()>,
{
    let mut got = 0u64;
    while got < len {
        match recv(r).await? {
            Frame::Raw(b) => {
                got += b.len() as u64;
                if got > len {
                    bail!("data overrun");
                }
                sink(&b)?;
            }
            Frame::Json(_) => bail!("expected a data frame"),
        }
    }
    Ok(())
}

pub async fn send_file<W: AsyncWrite + Unpin>(w: &mut W, path: &Path) -> Result<()> {
    let mut f = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("opening {}", path.display()))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        send_raw(w, &buf[..n]).await?;
    }
}

pub fn clean_rel(path: &str) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for c in Path::new(path).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => bail!("unsafe path '{path}'"),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("empty path");
    }
    Ok(out)
}

pub fn token_eq(a: &str, b: &str) -> bool {
    blake3::hash(a.as_bytes()) == blake3::hash(b.as_bytes())
}

type Streams = Arc<Mutex<HashMap<u32, mpsc::Sender<Vec<u8>>>>>;

pub async fn mux_acceptor(tunnel: TcpStream, listener: TcpListener) {
    let (tr, tw) = tunnel.into_split();
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(tunnel_writer(tw, rx));
    let streams: Streams = Arc::default();
    {
        let streams = streams.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut next = 1u32;
            while let Ok((conn, _)) = listener.accept().await {
                let id = next;
                next += 1;
                let (ltx, lrx) = mpsc::channel(32);
                streams.lock().await.insert(id, ltx);
                tokio::spawn(pump_local(id, conn, tx.clone(), lrx));
            }
        });
    }
    demux(tr, streams, None, tx).await;
}

pub async fn mux_dialer(tunnel: TcpStream, target: SocketAddr) {
    let (tr, tw) = tunnel.into_split();
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(tunnel_writer(tw, rx));
    demux(tr, Arc::default(), Some(target), tx).await;
}

async fn tunnel_writer(mut w: OwnedWriteHalf, mut rx: mpsc::Receiver<(u32, Vec<u8>)>) {
    while let Some((id, data)) = rx.recv().await {
        if w.write_u32(data.len() as u32).await.is_err()
            || w.write_u32(id).await.is_err()
            || w.write_all(&data).await.is_err()
            || w.flush().await.is_err()
        {
            break;
        }
    }
}

async fn pump_local(
    id: u32,
    local: TcpStream,
    tx: mpsc::Sender<(u32, Vec<u8>)>,
    mut rx: mpsc::Receiver<Vec<u8>>,
) {
    let _ = local.set_nodelay(true);
    let (mut lr, mut lw) = local.into_split();
    let up = async {
        let mut buf = vec![0u8; CHUNK];
        loop {
            match lr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send((id, buf[..n].to_vec())).await.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = tx.send((id, Vec::new())).await;
    };
    let down = async {
        while let Some(data) = rx.recv().await {
            if data.is_empty() || lw.write_all(&data).await.is_err() {
                break;
            }
        }
        let _ = lw.shutdown().await;
    };
    tokio::join!(up, down);
}

async fn demux(
    mut tr: OwnedReadHalf,
    streams: Streams,
    dial: Option<SocketAddr>,
    tx: mpsc::Sender<(u32, Vec<u8>)>,
) {
    loop {
        let len = match tr.read_u32().await {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        let id = match tr.read_u32().await {
            Ok(v) => v,
            Err(_) => break,
        };
        if len > MAX_FRAME {
            break;
        }
        let mut data = vec![0u8; len];
        if tr.read_exact(&mut data).await.is_err() {
            break;
        }
        let sender = streams.lock().await.get(&id).cloned();
        match sender {
            Some(s) => {
                if data.is_empty() {
                    streams.lock().await.remove(&id);
                }
                let _ = s.send(data).await;
            }
            None => {
                if let Some(addr) = dial {
                    if data.is_empty() {
                        continue;
                    }
                    match TcpStream::connect(addr).await {
                        Ok(conn) => {
                            let (ltx, lrx) = mpsc::channel(32);
                            let _ = ltx.send(data).await;
                            streams.lock().await.insert(id, ltx);
                            tokio::spawn(pump_local(id, conn, tx.clone(), lrx));
                        }
                        Err(_) => {
                            let _ = tx.send((id, Vec::new())).await;
                        }
                    }
                }
            }
        }
    }
}
