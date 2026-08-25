//! v2 authenticated-handshake conformance — the Rust reference implementation
//! must reproduce, byte-for-byte, the cross-SDK golden vectors in
//! `tests/conformance/vectors/adatp-v2-handshake-vectors.json`. This closes the
//! loop: the committed vectors cannot silently drift from the code, and every
//! other SDK's v2 runner replays the same file.

use adatp_core::crypto::ed25519::SigningKeyPair;
use adatp_core::crypto::CryptoError;
use adatp_core::session::handshake_v2::{
    client_verify_server_hello, finished_plaintext, sign_transcript, transcript_hash,
};
use serde_json::Value;

const VECTORS: &str =
    include_str!("../../tests/conformance/vectors/adatp-v2-handshake-vectors.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("v2 handshake vectors parse")
}

fn case<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("vector case '{id}' missing"))
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
fn arr32(s: &str) -> [u8; 32] {
    let mut a = [0u8; 32];
    a.copy_from_slice(&unhex(s));
    a
}
fn hexstr(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn transcript_hash_matches_vector() {
    let v = vectors();
    let c = case(&v, "handshake-v2-transcript-hash");
    let seed = arr32(c["input"]["server_seed_hex"].as_str().unwrap());
    let id = SigningKeyPair::from_seed(&seed);

    // The seed's public key is the pinned identity.
    assert_eq!(
        hexstr(&id.public_key_bytes()),
        c["input"]["spk_s_hex"].as_str().unwrap(),
        "seed → spk_s"
    );

    let epk_c = arr32(c["input"]["epk_c_hex"].as_str().unwrap());
    let epk_s = arr32(c["input"]["epk_s_hex"].as_str().unwrap());
    let th = transcript_hash(&epk_c, &epk_s, &id.public_key_bytes());
    assert_eq!(hexstr(&th), c["expected"]["transcript_hash_hex"].as_str().unwrap());
}

#[test]
fn server_hello_matches_vector_and_verifies() {
    let v = vectors();
    let c = case(&v, "handshake-v2-server-hello");
    let seed = arr32(c["input"]["server_seed_hex"].as_str().unwrap());
    let id = SigningKeyPair::from_seed(&seed);
    let epk_c = arr32(c["input"]["epk_c_hex"].as_str().unwrap());
    let epk_s = arr32(c["input"]["epk_s_hex"].as_str().unwrap());

    let (_th, sig) = sign_transcript(&id, &epk_c, &epk_s);
    assert_eq!(hexstr(&sig), c["expected"]["signature_hex"].as_str().unwrap(), "signature");

    // ServerHello wire = epk_s || spk_s || sig.
    let mut wire = Vec::new();
    wire.extend_from_slice(&epk_s);
    wire.extend_from_slice(&id.public_key_bytes());
    wire.extend_from_slice(&sig);
    assert_eq!(hexstr(&wire), c["expected"]["server_hello_hex"].as_str().unwrap(), "wire");

    // A client that pinned this identity accepts it.
    let pinned = id.public_key_bytes();
    assert!(client_verify_server_hello(&pinned, &epk_c, &wire).is_ok());
}

#[test]
fn wrong_pin_is_rejected() {
    let v = vectors();
    let c = case(&v, "handshake-v2-server-hello-wrong-pin");
    let pinned = arr32(c["input"]["pinned_spk_s_hex"].as_str().unwrap());
    let epk_c = arr32(c["input"]["epk_c_hex"].as_str().unwrap());
    let wire = unhex(c["input"]["server_hello_hex"].as_str().unwrap());
    assert!(matches!(
        client_verify_server_hello(&pinned, &epk_c, &wire),
        Err(CryptoError::InvalidKey)
    ));
}

#[test]
fn tampered_ephemeral_is_rejected() {
    let v = vectors();
    let c = case(&v, "handshake-v2-server-hello-tampered-ephemeral");
    let pinned = arr32(c["input"]["pinned_spk_s_hex"].as_str().unwrap());
    let epk_c = arr32(c["input"]["epk_c_hex"].as_str().unwrap());
    let wire = unhex(c["input"]["server_hello_hex"].as_str().unwrap());
    assert!(matches!(
        client_verify_server_hello(&pinned, &epk_c, &wire),
        Err(CryptoError::SignatureError)
    ));
}

#[test]
fn finished_plaintext_matches_vector() {
    let v = vectors();
    let c = case(&v, "handshake-v2-finished");
    let th = arr32(c["input"]["transcript_hash_hex"].as_str().unwrap());
    assert_eq!(
        hexstr(&finished_plaintext(&th)),
        c["expected"]["finished_plaintext_hex"].as_str().unwrap()
    );
}
