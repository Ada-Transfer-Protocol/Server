use anyhow::{Result, anyhow};
use tokio::net::TcpStream;
use adatp_core::transport::tcp::TcpTransport;
use adatp_core::codec::packet::{Packet, MessageType, PacketFlags};
use adatp_core::crypto::x25519::{KeyPair, diffie_hellman};
use bytes::Bytes;
use uuid::Uuid;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    address: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    println!("Connecting to {}...", args.address);

    let stream = TcpStream::connect(args.address).await?;
    let mut transport = TcpTransport::new(stream);

    // 1. Generate Client Keypair
    let client_keys = KeyPair::generate();
    
    // 2. Send HANDSHAKE_INIT
    let init_payload = Bytes::copy_from_slice(client_keys.public.as_bytes());
    let init_packet = Packet::new(
        MessageType::HandshakeInit,
        init_payload,
        Uuid::new_v4()
    );
    transport.write_packet(&init_packet).await?;
    println!("Sent HANDSHAKE_INIT");

    // 3. Receive HANDSHAKE_RESPONSE
    let response_packet = transport.read_packet().await?
        .ok_or(anyhow!("Connection closed during handshake"))?;
        
    if response_packet.header.msg_type != MessageType::HandshakeResponse {
        return Err(anyhow!("Expected HANDSHAKE_RESPONSE"));
    }
    
    let server_pub_key = response_packet.payload;
    if server_pub_key.len() != 32 {
        return Err(anyhow!("Invalid server public key length"));
    }
    println!("Received HANDSHAKE_RESPONSE (Server Public Key: {:?})", &server_pub_key[0..4]);

    // 4. Compute Shared Secret
    let shared_secret = diffie_hellman(client_keys.secret, &server_pub_key)
        .map_err(|e| anyhow!("DH Error: {:?}", e))?;

    // Derive Session Keys (HKDF)
    let session_keys = adatp_core::crypto::key_derivation::SessionKeys::derive(&shared_secret, &[0u8; 32]);
    let mut secure_session = adatp_core::session::secure_session::SecureSession::new(
        adatp_core::session::secure_session::Role::Client, 
        session_keys
    );

    // 5. Send HANDSHAKE_COMPLETE (Encrypted)
    let msg = b"Verification OK";
    let (ciphertext, tag, seq) = secure_session.encrypt(msg)
        .map_err(|e| anyhow!("Encryption error: {:?}", e))?;
        
    let mut complete_packet = Packet::new(
        MessageType::HandshakeComplete,
        Bytes::from(ciphertext),
        response_packet.header.session_id
    );
    complete_packet.header.flags = PacketFlags::ENCRYPTED;
    complete_packet.header.sequence = seq;
    complete_packet.auth_tag = Some(tag);
    
    transport.write_packet(&complete_packet).await?;
    println!("Sent HANDSHAKE_COMPLETE");

    // 6. Send a Text Message
    let text = b"Hello from Rust Client!";
    let (cipher_text, tag_text, seq_text) = secure_session.encrypt(text)
        .map_err(|e| anyhow!{"Encryption error: {:?}", e})?;
        
    let mut msg_packet = Packet::new(
        MessageType::TextMessage,
        Bytes::from(cipher_text),
        response_packet.header.session_id
    );
    msg_packet.header.flags = PacketFlags::ENCRYPTED;
    msg_packet.header.sequence = seq_text;
    msg_packet.auth_tag = Some(tag_text);
    
    transport.write_packet(&msg_packet).await?;
    println!("Sent Text Message");

    // 7. Receive Echo
    let echo = transport.read_packet().await?
        .ok_or(anyhow!("Connection closed during echo"))?;
        
    if echo.header.flags.contains(PacketFlags::ENCRYPTED) {
        let decrypted = secure_session.decrypt(&echo)
            .map_err(|e| anyhow!("Decryption failed: {:?}", e))?;
        println!("Received Echo: {} (Seq: {})", String::from_utf8_lossy(&decrypted), echo.header.sequence);
    }

    // 8. Disconnect
    let disconnect = Packet::new(
        MessageType::Disconnect,
        Bytes::new(),
        response_packet.header.session_id
    );
    transport.write_packet(&disconnect).await?;
    println!("Sent DISCONNECT");

    Ok(())
}
