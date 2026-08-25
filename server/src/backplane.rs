//! Multi-node routing backplane over Redis pub/sub.
//!
//! By default AdaTP routes rooms in-process (one node, a `DashMap` in [`Hub`]).
//! With `ADATP_BACKPLANE_URL=redis://host:port` set, every room broadcast is
//! also published to a shared Redis channel; each node subscribes and re-delivers
//! received messages to **its own** local connections. That makes rooms span an
//! arbitrary number of nodes behind a load balancer — the single-node ceiling the
//! roadmap called out.
//!
//! We speak RESP over a raw `tokio` TCP socket rather than pull in a Redis crate,
//! because the build is fully vendored/offline and no such crate is vendored.
//! Two connections are used: one for `PUBLISH`, one held in `SUBSCRIBE` mode.
//! A per-process node id tags every message so a node ignores its own echo
//! (Redis delivers published messages back to the publisher too).
//!
//! The routed payload is plaintext (same as the in-process hop — AdaTP is
//! hop-by-hop, not E2E; each receiving connection re-encrypts with its own
//! keys). Secure the Redis link at the network layer in production.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use uuid::Uuid;

use adatp_core::MessageType;

use crate::hub::{Hub, RouteMsg};

const CHANNEL: &str = "adatp:route";

/// Handle to the backplane. Dropping it does not stop the background tasks
/// (they live for the process); it exists mainly to expose the node id.
pub struct Backplane {
    node_id: Uuid,
}

impl Backplane {
    pub fn node_id(&self) -> Uuid {
        self.node_id
    }

    /// Connect to Redis at `url` (`redis://host:port`, or `host:port`), wire the
    /// publisher into `hub`, and start the subscriber. Returns `Err` if the
    /// initial connection fails, so a misconfigured URL is caught at boot; once
    /// started, both tasks reconnect on their own if the link drops.
    pub async fn start(url: &str, hub: Arc<Hub>) -> Result<Arc<Self>, String> {
        let addr = parse_addr(url);
        let node_id = Uuid::new_v4();

        // Fail fast on a bad address so boot surfaces the misconfiguration.
        let pub_stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("backplane: cannot reach Redis at {addr}: {e}"))?;

        // Publisher: drain (room, msg) from the hub and PUBLISH each.
        let (tx, rx) = mpsc::channel::<(String, RouteMsg)>(4096);
        hub.set_publisher(tx);
        tokio::spawn(publisher_task(addr.clone(), node_id, pub_stream, rx));

        // Subscriber: SUBSCRIBE and re-deliver others' messages to local conns.
        tokio::spawn(subscriber_task(addr, node_id, hub));

        Ok(Arc::new(Self { node_id }))
    }
}

/// `redis://host:port[/db]` or `host:port` → `host:port` (db/auth ignored).
fn parse_addr(url: &str) -> String {
    let s = url.strip_prefix("redis://").unwrap_or(url);
    let s = s.strip_prefix("rediss://").unwrap_or(s);
    // drop any trailing /db and any user:pass@ prefix
    let s = s.rsplit('@').next().unwrap_or(s);
    let s = s.split('/').next().unwrap_or(s);
    if s.contains(':') {
        s.to_string()
    } else {
        format!("{s}:6379")
    }
}

/// Frame an envelope: node(16) | msg_type(2 LE) | sender(16) | room_len(2 LE) | room | payload.
fn encode(node: Uuid, room: &str, msg: &RouteMsg) -> Vec<u8> {
    let room_bytes = room.as_bytes();
    let mut v = Vec::with_capacity(36 + room_bytes.len() + msg.payload.len());
    v.extend_from_slice(node.as_bytes());
    v.extend_from_slice(&(msg.msg_type as u16).to_le_bytes());
    v.extend_from_slice(msg.sender.as_bytes());
    v.extend_from_slice(&(room_bytes.len() as u16).to_le_bytes());
    v.extend_from_slice(room_bytes);
    v.extend_from_slice(&msg.payload);
    v
}

fn decode(buf: &[u8]) -> Option<(Uuid, String, RouteMsg)> {
    if buf.len() < 36 {
        return None;
    }
    let node = Uuid::from_slice(&buf[0..16]).ok()?;
    let mtype = u16::from_le_bytes([buf[16], buf[17]]);
    let sender = Uuid::from_slice(&buf[18..34]).ok()?;
    let room_len = u16::from_le_bytes([buf[34], buf[35]]) as usize;
    if buf.len() < 36 + room_len {
        return None;
    }
    let room = String::from_utf8(buf[36..36 + room_len].to_vec()).ok()?;
    let payload = Bytes::copy_from_slice(&buf[36 + room_len..]);
    Some((
        node,
        room,
        RouteMsg {
            sender,
            msg_type: MessageType::from(mtype),
            payload,
        },
    ))
}

/// Serialize a RESP2 command from binary-safe bulk-string args.
fn resp_command(args: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("*{}\r\n", args.len()).as_bytes());
    for a in args {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a);
        out.extend_from_slice(b"\r\n");
    }
    out
}

async fn publisher_task(
    addr: String,
    node_id: Uuid,
    initial: TcpStream,
    mut rx: mpsc::Receiver<(String, RouteMsg)>,
) {
    let mut stream = Some(initial);
    while let Some((room, msg)) = rx.recv().await {
        let frame = encode(node_id, &room, &msg);
        let cmd = resp_command(&[b"PUBLISH", CHANNEL.as_bytes(), &frame]);
        loop {
            if stream.is_none() {
                match TcpStream::connect(&addr).await {
                    Ok(s) => stream = Some(s),
                    Err(e) => {
                        log::warn!("backplane publisher: reconnect to {addr} failed: {e}");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                }
            }
            let s = stream.as_mut().unwrap();
            // Write the PUBLISH and read its integer reply to keep the socket synced.
            if s.write_all(&cmd).await.is_err() || read_publish_reply(s).await.is_err() {
                log::warn!("backplane publisher: write failed; reconnecting");
                stream = None;
                continue;
            }
            break;
        }
    }
}

/// Read and discard a single RESP reply line (PUBLISH returns `:<n>\r\n`).
async fn read_publish_reply(s: &mut TcpStream) -> std::io::Result<()> {
    let mut byte = [0u8; 1];
    // read until we consume one full line ending in \n
    loop {
        s.read_exact(&mut byte).await?;
        if byte[0] == b'\n' {
            return Ok(());
        }
    }
}

async fn subscriber_task(addr: String, node_id: Uuid, hub: Arc<Hub>) {
    loop {
        if let Err(e) = run_subscriber(&addr, node_id, &hub).await {
            log::warn!("backplane subscriber: {e}; reconnecting in 500ms");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn run_subscriber(addr: &str, node_id: Uuid, hub: &Arc<Hub>) -> std::io::Result<()> {
    let stream = TcpStream::connect(addr).await?;
    let (read_half, mut write_half) = stream.into_split();
    write_half
        .write_all(&resp_command(&[b"SUBSCRIBE", CHANNEL.as_bytes()]))
        .await?;
    let mut reader = BufReader::new(read_half);
    loop {
        match read_message(&mut reader).await? {
            Some(parts) if parts.len() == 3 && parts[0] == b"message" => {
                if let Some((node, room, msg)) = decode(&parts[2]) {
                    if node != node_id {
                        hub.broadcast_local(&room, msg);
                    }
                }
            }
            Some(_) => { /* subscribe confirmation / pings */ }
            None => return Ok(()), // connection closed → reconnect
        }
    }
}

/// Read one RESP value as a list of byte-strings. Handles the array-of-bulk
/// shape Redis pub/sub uses (`*3 message <chan> <payload>`); simple lines are
/// returned as a single-element list.
async fn read_message(
    reader: &mut BufReader<OwnedReadHalf>,
) -> std::io::Result<Option<Vec<Vec<u8>>>> {
    let line = match read_line(reader).await? {
        Some(l) => l,
        None => return Ok(None),
    };
    if line.is_empty() {
        return Ok(Some(vec![]));
    }
    match line[0] {
        b'*' => {
            let n: i64 = std::str::from_utf8(&line[1..])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(-1);
            if n < 0 {
                return Ok(Some(vec![]));
            }
            let mut items = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let header = match read_line(reader).await? {
                    Some(h) => h,
                    None => return Ok(None),
                };
                if header.first() == Some(&b'$') {
                    let len: i64 = std::str::from_utf8(&header[1..])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(-1);
                    if len < 0 {
                        items.push(Vec::new());
                        continue;
                    }
                    let mut data = vec![0u8; len as usize];
                    reader.read_exact(&mut data).await?;
                    let mut crlf = [0u8; 2];
                    reader.read_exact(&mut crlf).await?; // trailing \r\n
                    items.push(data);
                } else {
                    items.push(header[1..].to_vec());
                }
            }
            Ok(Some(items))
        }
        _ => Ok(Some(vec![line[1..].to_vec()])),
    }
}

/// Read a line terminated by `\n`, stripping the trailing `\r\n`. `None` at EOF.
async fn read_line(reader: &mut BufReader<OwnedReadHalf>) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Ok(None);
    }
    while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
        buf.pop();
    }
    Ok(Some(buf))
}
