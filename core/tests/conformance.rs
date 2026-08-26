//! AdaTP v1 conformance — replays the golden vectors against the Rust
//! reference implementation. The vectors file is the same JSON every SDK's
//! conformance runner consumes (docs/spec/appendix-test-vectors.md).

use adatp_core::codec::packet::{MessageType, Packet};
use adatp_core::crypto::key_derivation::SessionKeys;
use adatp_core::session::secure_session::{Role, SecureSession};
use bytes::Bytes;
use serde_json::Value;
use uuid::Uuid;

fn vectors() -> Value {
    let raw = include_str!("vectors.json");
    serde_json::from_str(raw).expect("vectors.json parses")
}

fn case<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("vector case '{id}' missing"))
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Builds a packet with the vector's fixed header fields.
fn build(msg_type: MessageType, payload: Vec<u8>, c: &Value) -> Packet {
    let session_id =
        Uuid::from_slice(&hex_decode(c["input"]["session_id"].as_str().unwrap())).unwrap();
    let mut p = Packet::new(msg_type, Bytes::from(payload), session_id);
    p.header.timestamp = c["input"]["timestamp_ms"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    p.header.sequence = c["input"]["sequence"].as_str().unwrap().parse().unwrap();
    p
}

fn derive_keys(c: &Value) -> SessionKeys {
    let secret = hex_decode(c["input"]["shared_secret_hex"].as_str().unwrap());
    SessionKeys::derive(&secret, &[0u8; 32])
}

#[test]
fn frame_plaintext_text() {
    let v = vectors();
    let c = case(&v, "frame-plaintext-text");
    let p = build(MessageType::TextMessage, b"Hello, AdaTP!".to_vec(), c);
    assert_eq!(
        hex_encode(&p.to_bytes()),
        c["expected_frame_hex"].as_str().unwrap()
    );

    // And back: decode the golden bytes.
    let decoded = Packet::from_bytes(Bytes::from(hex_decode(
        c["expected_frame_hex"].as_str().unwrap(),
    )))
    .unwrap();
    assert_eq!(decoded.header.msg_type, MessageType::TextMessage);
    assert_eq!(&decoded.payload[..], b"Hello, AdaTP!");
}

#[test]
fn frame_plaintext_joinroom() {
    let v = vectors();
    let c = case(&v, "frame-plaintext-joinroom");
    let p = build(MessageType::JoinRoom, b"lobby".to_vec(), c);
    assert_eq!(
        hex_encode(&p.to_bytes()),
        c["expected_frame_hex"].as_str().unwrap()
    );
}

#[test]
fn kdf_hkdf_sha256() {
    let v = vectors();
    let c = case(&v, "kdf-hkdf-sha256");
    let secret = hex_decode(c["input"]["shared_secret_hex"].as_str().unwrap());
    let keys = SessionKeys::derive(&secret, &[0u8; 32]);
    assert_eq!(
        hex_encode(&keys.client_write_key),
        c["expected"]["client_write_key"]
    );
    assert_eq!(
        hex_encode(&keys.server_write_key),
        c["expected"]["server_write_key"]
    );
    assert_eq!(
        hex_encode(&keys.client_iv_root),
        c["expected"]["client_iv_root"]
    );
    assert_eq!(
        hex_encode(&keys.server_iv_root),
        c["expected"]["server_iv_root"]
    );
}

#[test]
fn encrypted_text_client_to_server() {
    let v = vectors();
    let c = case(&v, "frame-encrypted-text-client");

    // Client-side encrypt must reproduce the golden ciphertext (seq 1).
    let keys = derive_keys(c);
    let mut client = SecureSession::new(Role::Client, keys);
    let mut p = build(MessageType::TextMessage, Vec::new(), c);
    let (ciphertext, tag) = client.encrypt(b"secret message", &mut p.header).unwrap();
    assert_eq!(p.header.sequence, 1);
    assert_eq!(hex_encode(&ciphertext), c["expected"]["ciphertext_hex"]);
    assert_eq!(hex_encode(&tag), c["expected"]["auth_tag_hex"]);

    p.payload = Bytes::from(ciphertext);
    p.auth_tag = Some(tag);
    assert_eq!(
        hex_encode(&p.to_bytes()),
        c["expected"]["frame_hex"].as_str().unwrap()
    );

    // Server-side decrypt of the golden frame.
    let keys = derive_keys(c);
    let mut server = SecureSession::new(Role::Server, keys);
    let decoded = Packet::from_bytes(Bytes::from(hex_decode(
        c["expected"]["frame_hex"].as_str().unwrap(),
    )))
    .unwrap();
    assert_eq!(server.decrypt(&decoded).unwrap(), b"secret message");
}

#[test]
fn encrypted_gamestate_server_to_client() {
    let v = vectors();
    let c = case(&v, "frame-encrypted-gamestate-server");
    let plaintext = br#"{"board":[1,0,2],"turn":"p1"}"#;

    // The vector is server→client at seq 2: burn seq 1 first.
    let mut server = SecureSession::new(Role::Server, derive_keys(c));
    let mut burn = Packet::new(MessageType::TextMessage, Bytes::new(), Uuid::nil());
    let _ = server.encrypt(b"x", &mut burn.header).unwrap(); // seq 1
    let mut p = build(MessageType::GameState, Vec::new(), c);
    let (ciphertext, tag) = server.encrypt(plaintext, &mut p.header).unwrap();
    assert_eq!(p.header.sequence, 2);
    assert_eq!(hex_encode(&ciphertext), c["expected"]["ciphertext_hex"]);
    assert_eq!(hex_encode(&tag), c["expected"]["auth_tag_hex"]);

    // Client decrypts the golden frame.
    let mut client = SecureSession::new(Role::Client, derive_keys(c));
    let decoded = Packet::from_bytes(Bytes::from(hex_decode(
        c["expected"]["frame_hex"].as_str().unwrap(),
    )))
    .unwrap();
    assert_eq!(decoded.header.msg_type, MessageType::GameState);
    assert_eq!(client.decrypt(&decoded).unwrap(), plaintext);
}

#[test]
fn reject_bad_magic() {
    let v = vectors();
    let c = case(&v, "reject-bad-magic");
    let frame = hex_decode(c["input"]["frame_hex"].as_str().unwrap());
    assert!(Packet::from_bytes(Bytes::from(frame)).is_err());
}

#[test]
fn reject_short_header() {
    let v = vectors();
    let c = case(&v, "reject-short-header");
    let frame = hex_decode(c["input"]["frame_hex"].as_str().unwrap());
    assert!(Packet::from_bytes(Bytes::from(frame)).is_err());
}

#[test]
fn reject_tampered_tag() {
    let v = vectors();
    let c = case(&v, "frame-encrypted-text-client");
    let mut frame = hex_decode(c["expected"]["frame_hex"].as_str().unwrap());
    let last = frame.len() - 1;
    frame[last] ^= 0x01; // flip one bit of the auth tag

    let decoded = Packet::from_bytes(Bytes::from(frame)).unwrap();
    let mut server = SecureSession::new(Role::Server, derive_keys(c));
    assert!(server.decrypt(&decoded).is_err(), "tampered tag must fail");
}

#[test]
fn codec_roundtrip_all_types() {
    // Every registered type must survive encode→decode unchanged.
    for raw in [
        0x0001u16, 0x0002, 0x0003, 0x0010, 0x0013, 0x0014, 0x0020, 0x0030, 0x0031, 0x0032, 0x0033,
        0x0034, 0x0040, 0x0044, 0x0045, 0x0050, 0x0060, 0x0061, 0x0070, 0x0071, 0x0072, 0x0080,
        0x0081, 0x0090, 0x0093, 0x00A0, 0x00A1, 0x00FF,
    ] {
        let t = MessageType::from(raw);
        assert_ne!(t, MessageType::Unknown, "0x{raw:04x} must be a known type");
        let p = Packet::new(t, Bytes::from_static(b"x"), Uuid::nil());
        let decoded = Packet::from_bytes(p.to_bytes()).unwrap();
        assert_eq!(decoded.header.msg_type, t);
    }
}
