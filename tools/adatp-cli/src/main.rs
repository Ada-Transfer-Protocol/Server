use anyhow::{anyhow, Result};
use adatp_core::codec::packet::{MessageType, Packet};
use adatp_core::crypto::x25519::{diffie_hellman, KeyPair};
use bytes::Bytes;
use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// AdaTP protocol test tool.
///
/// Connects over WebSocket, performs the X25519 handshake, optionally logs
/// in, and reports each step. Useful for verifying a deployment end to end.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server URL or host:port (ws://host:port/ws is derived automatically)
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    address: String,

    #[arg(short, long)]
    username: Option<String>,

    #[arg(short, long)]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let url = if args.address.starts_with("ws://") || args.address.starts_with("wss://") {
        args.address.clone()
    } else {
        format!("ws://{}/ws", args.address)
    };

    println!("Connecting to {url} ...");
    let (ws, _) = tokio::time::timeout(std::time::Duration::from_secs(10), connect_async(&url))
        .await
        .map_err(|_| anyhow!("Timed out connecting to {url} — is an AdaTP server listening there?"))??;
    let (mut tx, mut rx) = ws.split();

    let session_id = Uuid::new_v4();

    // Helper: read the next AdaTP packet, skipping WS control frames.
    // Times out with a diagnostic instead of hanging forever when the far
    // end is not actually an AdaTP server (e.g. a dev server on the port).
    async fn next_packet(
        rx: &mut (impl StreamExt<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    ) -> Result<Packet> {
        let deadline = std::time::Duration::from_secs(10);
        loop {
            let msg = tokio::time::timeout(deadline, rx.next())
                .await
                .map_err(|_| anyhow!(
                    "No AdaTP reply within 10s — the endpoint accepted the WebSocket \
                     but does not speak AdaTP (wrong port or a different server?)"
                ))?;
            match msg {
                Some(m) => match m? {
                    Message::Binary(data) => {
                        return Packet::from_bytes(Bytes::from(data))
                            .map_err(|e| anyhow!("Malformed packet: {e}"));
                    }
                    Message::Close(_) => return Err(anyhow!("Connection closed")),
                    _ => {}
                },
                None => return Err(anyhow!("Connection closed")),
            }
        }
    }

    // 1. Client key pair
    let client_keys = KeyPair::generate();

    // 2. HANDSHAKE_INIT
    let init_packet = Packet::new(
        MessageType::HandshakeInit,
        Bytes::copy_from_slice(client_keys.public.as_bytes()),
        session_id,
    );
    tx.send(Message::Binary(init_packet.to_bytes().to_vec())).await?;
    println!("Sent HANDSHAKE_INIT");

    // 3. HANDSHAKE_RESPONSE
    let response_packet = next_packet(&mut rx).await?;
    if response_packet.header.msg_type != MessageType::HandshakeResponse {
        return Err(anyhow!("Expected HANDSHAKE_RESPONSE"));
    }
    let server_pub_key = response_packet.payload.clone();
    if server_pub_key.len() != 32 {
        return Err(anyhow!("Invalid server public key length"));
    }
    println!("Received HANDSHAKE_RESPONSE");

    // 4. Shared secret & session keys
    let shared_secret = diffie_hellman(client_keys.secret, &server_pub_key)
        .map_err(|e| anyhow!("DH error: {:?}", e))?;
    let session_keys =
        adatp_core::crypto::key_derivation::SessionKeys::derive(&shared_secret, &[0u8; 32]);
    let mut secure_session = adatp_core::session::secure_session::SecureSession::new(
        adatp_core::session::secure_session::Role::Client,
        session_keys,
    );

    // 5. HANDSHAKE_COMPLETE
    let mut complete_packet =
        Packet::new(MessageType::HandshakeComplete, Bytes::new(), session_id);
    let (ciphertext, tag) = secure_session
        .encrypt(b"Verification OK", &mut complete_packet.header)
        .map_err(|e| anyhow!("Encryption error: {:?}", e))?;
    complete_packet.payload = Bytes::from(ciphertext);
    complete_packet.auth_tag = Some(tag);
    tx.send(Message::Binary(complete_packet.to_bytes().to_vec())).await?;
    println!("Sent HANDSHAKE_COMPLETE -> Secure session established 🔒");

    // 6. Login (optional)
    if let (Some(u), Some(p)) = (args.username, args.password) {
        println!("Attempting login as '{u}'...");

        let login_json = serde_json::json!({
            "username": u,
            "password": p,
            "device_id": "cli-tool"
        });
        let mut login_pkt = Packet::new(MessageType::AuthRequest, Bytes::new(), session_id);
        let (cipher, tag) =
            secure_session.encrypt(&serde_json::to_vec(&login_json)?, &mut login_pkt.header)?;
        login_pkt.payload = Bytes::from(cipher);
        login_pkt.auth_tag = Some(tag);
        tx.send(Message::Binary(login_pkt.to_bytes().to_vec())).await?;

        let resp = next_packet(&mut rx).await?;
        match resp.header.msg_type {
            MessageType::AuthSuccess => {
                let decrypted = secure_session.decrypt(&resp)?;
                println!("✅ Login OK: {}", String::from_utf8_lossy(&decrypted));
            }
            MessageType::AuthFailure => {
                let decrypted = secure_session.decrypt(&resp)?;
                println!("❌ Login failed: {}", String::from_utf8_lossy(&decrypted));
            }
            other => println!("❌ Expected auth result, got {:?}", other),
        }
    } else {
        println!("Skipping login (no credentials provided)");
    }

    // 7. Disconnect
    let disconnect = Packet::new(MessageType::Disconnect, Bytes::new(), session_id);
    tx.send(Message::Binary(disconnect.to_bytes().to_vec())).await?;
    let _ = tx.send(Message::Close(None)).await;
    println!("Disconnected.");

    Ok(())
}
