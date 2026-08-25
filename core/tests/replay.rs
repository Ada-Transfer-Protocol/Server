//! Integration coverage for AdaTP replay protection (review Finding 1).
//!
//! Exercises the public `SecureSession` API end to end: a replayed or older
//! sequence number MUST be rejected with `CryptoError::ReplayDetected`, while a
//! fresh (strictly higher) sequence is accepted. Gap tolerance is also checked
//! — an ordered stream that skips sequence numbers (e.g. a dropped frame) still
//! advances the monotonic highest-seen window rather than wedging.

use adatp_core::codec::packet::{MessageType, Packet};
use adatp_core::crypto::key_derivation::SessionKeys;
use adatp_core::crypto::CryptoError;
use adatp_core::session::secure_session::{Role, SecureSession};
use bytes::Bytes;
use uuid::Uuid;

fn pair() -> (SecureSession, SecureSession) {
    let secret = [0x5au8; 32];
    let client = SecureSession::new(Role::Client, SessionKeys::derive(&secret, &[0u8; 32]));
    let server = SecureSession::new(Role::Server, SessionKeys::derive(&secret, &[0u8; 32]));
    (client, server)
}

/// Encrypt on the client and assemble the wire packet the server will see.
fn seal(client: &mut SecureSession, msg: &[u8]) -> Packet {
    let mut p = Packet::new(MessageType::TextMessage, Bytes::new(), Uuid::nil());
    let (ciphertext, tag) = client.encrypt(msg, &mut p.header).unwrap();
    p.payload = Bytes::from(ciphertext);
    p.auth_tag = Some(tag);
    p
}

#[test]
fn replay_is_rejected_and_fresh_accepted() {
    let (mut client, mut server) = pair();

    let p1 = seal(&mut client, b"alpha"); // seq 1
    let p2 = seal(&mut client, b"bravo"); // seq 2
    let p3 = seal(&mut client, b"charlie"); // seq 3

    assert_eq!(server.decrypt(&p1).unwrap(), b"alpha");
    assert_eq!(server.decrypt(&p2).unwrap(), b"bravo");

    // Replaying p1 (older) and p2 (the highest accepted) are both dropped.
    assert!(matches!(server.decrypt(&p1), Err(CryptoError::ReplayDetected)));
    assert!(matches!(server.decrypt(&p2), Err(CryptoError::ReplayDetected)));

    // The next fresh sequence still flows.
    assert_eq!(server.decrypt(&p3).unwrap(), b"charlie");
}

#[test]
fn sequence_gap_is_tolerated_then_locks_out_older() {
    let (mut client, mut server) = pair();

    let p1 = seal(&mut client, b"one"); // seq 1
    let _p2 = seal(&mut client, b"two"); // seq 2 (simulate this frame being lost)
    let p3 = seal(&mut client, b"three"); // seq 3

    // Accept seq 1, then jump straight to seq 3 (seq 2 never arrived).
    assert_eq!(server.decrypt(&p1).unwrap(), b"one");
    assert_eq!(server.decrypt(&p3).unwrap(), b"three");

    // The skipped seq 2, arriving late, is now older than the window → dropped.
    assert!(matches!(server.decrypt(&_p2), Err(CryptoError::ReplayDetected)));
}
