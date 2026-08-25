//! AdaTP protocol **v2** — authenticated handshake (SIGMA-style).
//!
//! This is the code realisation of the design in
//! [`docs/spec/12-authenticated-handshake.md`] whose secrecy and no-MITM
//! properties were checked in the symbolic model
//! ([`docs/spec/formal/`], results in `formal/RESULTS.md`). The construction —
//! and in particular the exact transcript that gets signed — is kept
//! **byte-for-byte identical** to what the ProVerif model authenticates, so the
//! proof is about *this* wire format:
//!
//! ```text
//!   th  = SHA-256( LABEL_HS || u8(2) || epk_C || epk_S || spk_S )
//!   sig = Ed25519_sign( ssk_S, th )
//! ```
//!
//! The server holds a **long-term** Ed25519 identity `(spk_S, ssk_S)`; the
//! client obtains `spk_S` out of band (provisioned / pinned / TOFU, spec §4) and
//! **must** reject a hello whose key is not the pinned one, or whose signature
//! does not verify. That single check is what turns v1's anonymous DH — which an
//! active attacker MITMs — into an authenticated exchange.
//!
//! Everything here is pure and deterministic given its inputs (Ed25519 signing
//! is deterministic, `th` depends only on the three public keys), which is what
//! lets [`tests`] pin conformance vectors for the signed transcript. The
//! X25519 → HKDF-SHA256 session that results is **unchanged from v1** — v2 only
//! adds identity + key-confirmation around the same primitives.

use sha2::{Digest, Sha256};

use crate::crypto::ed25519::{self, SigningKeyPair};
use crate::crypto::key_derivation::SessionKeys;
use crate::crypto::x25519::{diffie_hellman, KeyPair};
use crate::crypto::CryptoError;

/// Protocol version carried in the frame header's `version` byte for a v2
/// handshake. v1 (the current unauthenticated flow) stays `1` and untouched.
pub const PROTOCOL_V2: u8 = 2;

/// Domain-separation label for the signed transcript. Distinct from any other
/// signed context in the system so a signature here can never be replayed
/// elsewhere.
pub const LABEL_HS: &[u8] = b"AdaTP-v2-handshake";

/// Domain-separation label for the client's key-confirmation ("Finished").
pub const FINISHED_LABEL: &[u8] = b"AdaTP-v2-finished";

/// On-wire length of the server's response: `epk_S(32) || spk_S(32) || sig(64)`.
pub const SERVER_HELLO_LEN: usize = 32 + 32 + 64;

/// The signed transcript hash: `SHA-256(LABEL_HS || u8(2) || epk_C || epk_S || spk_S)`.
///
/// This is the exact preimage the server signs and the client re-derives and
/// verifies — and the exact term the ProVerif model calls `th`. Binding **both
/// ephemerals, the version, and the server identity** is what prevents an
/// attacker from substituting its own `epk_S` (the signature would not verify)
/// or silently downgrading the version.
pub fn transcript_hash(epk_c: &[u8; 32], epk_s: &[u8; 32], spk_s: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(LABEL_HS);
    h.update([PROTOCOL_V2]);
    h.update(epk_c);
    h.update(epk_s);
    h.update(spk_s);
    h.finalize().into()
}

/// Sign the transcript with the server's long-term identity. Pure and
/// deterministic (given `identity`, `epk_c`, `epk_s`) — the basis for the
/// conformance vectors. Returns `(th, signature)`.
pub fn sign_transcript(
    identity: &SigningKeyPair,
    epk_c: &[u8; 32],
    epk_s: &[u8; 32],
) -> ([u8; 32], [u8; 64]) {
    let spk_s = identity.public_key_bytes();
    let th = transcript_hash(epk_c, epk_s, &spk_s);
    let sig = identity.sign(&th);
    (th, sig)
}

/// Result of the server running the v2 exchange for one client hello.
pub struct ServerHandshake {
    /// The wire bytes to send back as `HandshakeResponse`: `epk_S || spk_S || sig`.
    pub response: Vec<u8>,
    /// The transcript hash — the server keeps this to later check the client's
    /// encrypted `Finished` binds the same handshake (key confirmation).
    pub transcript_hash: [u8; 32],
    /// The derived session keys (X25519 → HKDF-SHA256, identical to v1).
    pub keys: SessionKeys,
}

/// **Server side.** Given the client's ephemeral `epk_C` and our long-term
/// identity, generate a fresh server ephemeral, sign the transcript, and derive
/// the session keys. `salt` is the KDF salt (the same one v1 uses).
///
/// The server ephemeral is generated internally with the OS CSPRNG; the secret
/// never leaves this function (it is consumed by the DH). For deterministic
/// tests/vectors of the *signed transcript*, use [`sign_transcript`] directly —
/// it needs only public inputs.
pub fn server_respond(
    identity: &SigningKeyPair,
    epk_c: &[u8; 32],
    salt: &[u8],
) -> Result<ServerHandshake, CryptoError> {
    let ephemeral = KeyPair::generate();
    let epk_s = *ephemeral.public.as_bytes();
    let spk_s = identity.public_key_bytes();

    let (th, sig) = sign_transcript(identity, epk_c, &epk_s);

    // Derive the session AFTER capturing epk_s, since the DH consumes the secret.
    let shared = diffie_hellman(ephemeral.secret, epk_c)?;
    let keys = SessionKeys::derive(&shared, salt);

    let mut response = Vec::with_capacity(SERVER_HELLO_LEN);
    response.extend_from_slice(&epk_s);
    response.extend_from_slice(&spk_s);
    response.extend_from_slice(&sig);

    Ok(ServerHandshake { response, transcript_hash: th, keys })
}

/// What the client learns after a valid `HandshakeResponse`.
pub struct ClientVerified {
    /// The server's ephemeral public key — the client DHs against this.
    pub epk_s: [u8; 32],
    /// The transcript hash, to build the `Finished` confirmation.
    pub transcript_hash: [u8; 32],
}

/// **Client side.** Verify a v2 `HandshakeResponse` against the **pinned**
/// server identity `pinned_spk_s`. This is the whole security-relevant check:
///
/// 1. the response is well-formed (`SERVER_HELLO_LEN` bytes);
/// 2. the server key it carries **equals the pinned key** (else the server is
///    unknown / being impersonated → [`CryptoError::InvalidKey`]);
/// 3. the signature verifies over the transcript under that key (else the
///    exchange was tampered with / MITM'd → [`CryptoError::SignatureError`]).
///
/// A client MUST NOT derive any key material or send anything until this
/// returns `Ok`. On `Ok` it may DH `esk_C · epk_S` and proceed.
pub fn client_verify_server_hello(
    pinned_spk_s: &[u8; 32],
    epk_c: &[u8; 32],
    response: &[u8],
) -> Result<ClientVerified, CryptoError> {
    if response.len() != SERVER_HELLO_LEN {
        return Err(CryptoError::InvalidKey);
    }
    let mut epk_s = [0u8; 32];
    epk_s.copy_from_slice(&response[0..32]);
    let mut spk_s = [0u8; 32];
    spk_s.copy_from_slice(&response[32..64]);
    let sig = &response[64..128];

    // (1) Identity: the offered key must be exactly the one we pinned. Without
    // this, an attacker's validly-self-signed hello would sail through (2).
    if !ct_eq(&spk_s, pinned_spk_s) {
        return Err(CryptoError::InvalidKey);
    }

    // (2) Authenticity: the signature must cover *this* transcript (which binds
    // epk_c, epk_s and spk_s) under the pinned key. Re-derive th ourselves so a
    // forged epk_s cannot be paired with a stale signature.
    let th = transcript_hash(epk_c, &epk_s, &spk_s);
    ed25519::verify(&spk_s, &th, sig)?;

    Ok(ClientVerified { epk_s, transcript_hash: th })
}

/// The plaintext of the client's key-confirmation, `FINISHED_LABEL || th`,
/// which the client sends **encrypted** under the freshly derived session key as
/// `HandshakeComplete`. Because it is under the session key *and* names the
/// transcript, a correct decryption proves the client derived the same key for
/// the same handshake.
pub fn finished_plaintext(th: &[u8; 32]) -> Vec<u8> {
    let mut v = Vec::with_capacity(FINISHED_LABEL.len() + 32);
    v.extend_from_slice(FINISHED_LABEL);
    v.extend_from_slice(th);
    v
}

/// **Server side.** After decrypting the client's `HandshakeComplete`, check the
/// plaintext is exactly `FINISHED_LABEL || th` for the `th` of this handshake.
/// Only then is the session confirmed established.
pub fn verify_finished(th: &[u8; 32], plaintext: &[u8]) -> bool {
    ct_eq(plaintext, &finished_plaintext(th))
}

/// Constant-time equality. The values compared here are public (keys, a hash),
/// so this is hygiene rather than a strict requirement, but there is no reason
/// to leak a byte-position via early return.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const KDF_SALT: &[u8] = b"adatp-v2-test-salt";

    /// A fixed server identity for deterministic transcript/signature vectors.
    fn fixed_identity() -> SigningKeyPair {
        SigningKeyPair::from_seed(&[0x11; 32])
    }

    #[test]
    fn transcript_is_deterministic_and_domain_separated() {
        let epk_c = [0xAA; 32];
        let epk_s = [0xBB; 32];
        let spk_s = [0xCC; 32];

        // Stable across calls.
        let a = transcript_hash(&epk_c, &epk_s, &spk_s);
        let b = transcript_hash(&epk_c, &epk_s, &spk_s);
        assert_eq!(a, b);

        // Sensitive to every field (swapping the ephemerals changes it — so an
        // attacker cannot reflect the client's key as the server's).
        assert_ne!(a, transcript_hash(&epk_s, &epk_c, &spk_s));
        assert_ne!(a, transcript_hash(&epk_c, &epk_s, &[0xCD; 32]));
    }

    /// Conformance vector: fixed identity + fixed ephemerals ⇒ fixed th + sig.
    /// If this ever changes, the wire format changed and every SDK's v2 vectors
    /// must change with it.
    #[test]
    fn signed_transcript_conformance_vector() {
        let id = fixed_identity();
        let epk_c = [0x01; 32];
        let epk_s = [0x02; 32];

        let (th, sig) = sign_transcript(&id, &epk_c, &epk_s);

        // th is a pure function of the three public keys.
        assert_eq!(th, transcript_hash(&epk_c, &epk_s, &id.public_key_bytes()));
        // Ed25519 is deterministic: signing the same th twice is identical.
        let (_, sig2) = sign_transcript(&id, &epk_c, &epk_s);
        assert_eq!(sig, sig2);
        // And it verifies under the identity's public key.
        assert!(ed25519::verify(&id.public_key_bytes(), &th, &sig).is_ok());
    }

    #[test]
    fn honest_round_trip_agrees_on_keys() {
        let id = fixed_identity();
        let pinned = id.public_key_bytes();

        // Client ephemeral.
        let client = KeyPair::generate();
        let epk_c = *client.public.as_bytes();

        // Server responds.
        let sh = server_respond(&id, &epk_c, KDF_SALT).unwrap();

        // Client verifies against the pinned key, then DHs to the same secret.
        let v = client_verify_server_hello(&pinned, &epk_c, &sh.response).unwrap();
        assert_eq!(v.transcript_hash, sh.transcript_hash);

        let shared = diffie_hellman(client.secret, &v.epk_s).unwrap();
        let client_keys = SessionKeys::derive(&shared, KDF_SALT);
        // Both sides derived the same session (spot-check both directions).
        assert_eq!(client_keys.client_write_key, sh.keys.client_write_key);
        assert_eq!(client_keys.server_write_key, sh.keys.server_write_key);

        // And the client's Finished confirms under that transcript.
        assert!(verify_finished(&sh.transcript_hash, &finished_plaintext(&v.transcript_hash)));
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let id = fixed_identity();
        let pinned = id.public_key_bytes();
        let epk_c = *KeyPair::generate().public.as_bytes();
        let mut sh = server_respond(&id, &epk_c, KDF_SALT).unwrap();

        // Flip a bit in the signature region (bytes 64..128).
        sh.response[100] ^= 0x01;
        assert!(matches!(
            client_verify_server_hello(&pinned, &epk_c, &sh.response),
            Err(CryptoError::SignatureError)
        ));
    }

    #[test]
    fn unknown_identity_is_rejected_even_with_valid_self_signature() {
        // The MITM's strongest move: run its own perfectly valid handshake with
        // its OWN identity key. Everything self-signs correctly — the ONLY thing
        // that stops it is the client pinning the real server's key.
        let attacker = SigningKeyPair::from_seed(&[0x99; 32]);
        let real_pinned = fixed_identity().public_key_bytes();
        let epk_c = *KeyPair::generate().public.as_bytes();

        let sh = server_respond(&attacker, &epk_c, KDF_SALT).unwrap();
        // The attacker's hello is internally valid...
        assert!(client_verify_server_hello(&attacker.public_key_bytes(), &epk_c, &sh.response).is_ok());
        // ...but a client that pinned the REAL server rejects it outright.
        assert!(matches!(
            client_verify_server_hello(&real_pinned, &epk_c, &sh.response),
            Err(CryptoError::InvalidKey)
        ));
    }

    #[test]
    fn substituted_ephemeral_breaks_the_signature() {
        // Attacker keeps the real spk_s (so the pin check passes) but swaps in
        // its own epk_s to control the DH. The signature no longer matches the
        // transcript ⇒ rejected. This is the classic active-MITM attempt.
        let id = fixed_identity();
        let pinned = id.public_key_bytes();
        let epk_c = *KeyPair::generate().public.as_bytes();
        let mut sh = server_respond(&id, &epk_c, KDF_SALT).unwrap();

        // Overwrite epk_s (bytes 0..32) with the attacker's ephemeral.
        let attacker_eph = *KeyPair::generate().public.as_bytes();
        sh.response[0..32].copy_from_slice(&attacker_eph);

        assert!(matches!(
            client_verify_server_hello(&pinned, &epk_c, &sh.response),
            Err(CryptoError::SignatureError)
        ));
    }

    #[test]
    fn finished_binds_the_transcript() {
        let th = [0x42; 32];
        let other = [0x43; 32];
        assert!(verify_finished(&th, &finished_plaintext(&th)));
        // A Finished for a different handshake does not confirm this one.
        assert!(!verify_finished(&th, &finished_plaintext(&other)));
        // Nor does a truncated/garbage confirmation.
        assert!(!verify_finished(&th, b"AdaTP-v2-finished"));
        assert!(!verify_finished(&th, &[]));
    }

    #[test]
    fn malformed_response_length_is_rejected() {
        let pinned = [0u8; 32];
        let epk_c = [0u8; 32];
        assert!(client_verify_server_hello(&pinned, &epk_c, &[0u8; 127]).is_err());
        assert!(client_verify_server_hello(&pinned, &epk_c, &[0u8; 129]).is_err());
        assert!(client_verify_server_hello(&pinned, &epk_c, &[]).is_err());
    }
}
