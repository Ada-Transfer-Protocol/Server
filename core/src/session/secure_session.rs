use crate::codec::packet::{Packet, PacketFlags, PacketHeader};
use crate::crypto::CryptoError;
use crate::crypto::{aes_gcm::Cipher, key_derivation::SessionKeys};

pub enum Role {
    Client,
    Server,
}

pub struct SecureSession {
    role: Role,
    keys: SessionKeys,
    cipher_client: Cipher,
    cipher_server: Cipher,

    // Sequence numbers strictly increasing
    my_sequence: u64,
    peer_sequence: u64,

    /// When true (protocol **v2**), the 45-byte frame header is bound as the
    /// AEAD additional authenticated data, so header fields (msg_type, sequence,
    /// session_id, flags…) are tamper-evident. v1 sessions use empty AAD, which
    /// keeps their golden vectors byte-identical.
    bind_aad: bool,
}

impl SecureSession {
    /// A v1 session: empty AAD (wire-compatible with the v1 golden vectors).
    pub fn new(role: Role, keys: SessionKeys) -> Self {
        Self::with_aad(role, keys, false)
    }

    /// A **v2** session: the frame header is authenticated as AEAD AAD.
    pub fn new_v2(role: Role, keys: SessionKeys) -> Self {
        Self::with_aad(role, keys, true)
    }

    fn with_aad(role: Role, keys: SessionKeys, bind_aad: bool) -> Self {
        let cipher_client = Cipher::new(keys.client_write_key);
        let cipher_server = Cipher::new(keys.server_write_key);

        Self {
            role,
            keys,
            cipher_client,
            cipher_server,
            my_sequence: 1, // Start from 1, 0 might be used for handshake packets if unencrypted
            peer_sequence: 1,
            bind_aad,
        }
    }

    /// Encrypts `plaintext` for the frame described by `header`. Fills in
    /// `header.sequence`, `header.length` and the `ENCRYPTED` flag; for a v2
    /// session the finalized header is bound as AEAD AAD (so it cannot be
    /// altered in flight). Returns `(ciphertext, tag)`; the sequence used is
    /// left in `header.sequence`.
    ///
    /// IV = IV_root XOR sequence in the low 8 bytes (spec 08-security), same
    /// construction as v1.
    pub fn encrypt(
        &mut self,
        plaintext: &[u8],
        header: &mut PacketHeader,
    ) -> Result<(Vec<u8>, [u8; 16]), CryptoError> {
        let seq = self.my_sequence;
        header.sequence = seq;
        header.length = plaintext.len() as u32;
        header.flags |= PacketFlags::ENCRYPTED;

        let iv = self.compute_iv(seq, &self.role);
        let aad_bytes = header.header_bytes();
        let aad: &[u8] = if self.bind_aad { &aad_bytes } else { &[] };

        let (ciphertext, tag) = match self.role {
            Role::Client => self.cipher_client.encrypt(&iv, plaintext, aad)?,
            Role::Server => self.cipher_server.encrypt(&iv, plaintext, aad)?,
        };

        self.my_sequence += 1;
        Ok((ciphertext, tag))
    }

    // Decrypts a packet payload
    pub fn decrypt(&mut self, packet: &Packet) -> Result<Vec<u8>, CryptoError> {
        if !packet.header.flags.contains(PacketFlags::ENCRYPTED) {
            // If not encrypted, arguably we should allow pass-through or fail depending on strictness.
            // For SecureSession, we expect encryption.
            return Ok(packet.payload.to_vec());
        }

        let seq = packet.header.sequence;

        // Replay protection (spec 08-security, "Sequencing"): sequence numbers
        // are strictly increasing, and a packet whose sequence has already been
        // accepted — or is older than the highest accepted — MUST be dropped.
        // We keep a monotonic highest-accepted counter (`peer_sequence`) and
        // reject anything below it. Over an ordered TCP/WebSocket stream this is
        // non-breaking (legitimate traffic never regresses); it defeats replays
        // and reordering attacks. The counter is advanced ONLY after the AEAD
        // tag verifies, so a forged or garbage sequence number cannot wedge the
        // window and lock out subsequent genuine packets (a DoS).
        if seq < self.peer_sequence {
            return Err(CryptoError::ReplayDetected);
        }

        let peer_role = match self.role {
            Role::Client => Role::Server,
            Role::Server => Role::Client,
        };

        let iv = self.compute_iv(seq, &peer_role);
        let tag = packet.auth_tag.ok_or(CryptoError::EncryptionError)?; // Tag missing

        // v2 binds the received header as AAD: any tampering with msg_type,
        // sequence, session_id or flags fails the tag below. v1 uses empty AAD.
        let aad_bytes = packet.header.header_bytes();
        let aad: &[u8] = if self.bind_aad { &aad_bytes } else { &[] };

        let plaintext = match self.role {
            Role::Client => self
                .cipher_server
                .decrypt(&iv, &packet.payload, &tag, aad)?,
            Role::Server => self
                .cipher_client
                .decrypt(&iv, &packet.payload, &tag, aad)?,
        };

        // Authenticated successfully — advance the replay window past this seq.
        self.peer_sequence = seq.saturating_add(1);

        Ok(plaintext)
    }

    fn compute_iv(&self, sequence: u64, sender_role: &Role) -> [u8; 12] {
        let root = match sender_role {
            Role::Client => self.keys.client_iv_root,
            Role::Server => self.keys.server_iv_root,
        };

        let mut iv = root;
        let seq_bytes = sequence.to_le_bytes(); // 8 bytes

        // XOR the last 8 bytes of IV (bytes 4..12) with sequence
        // This is a common pattern (e.g. TLS 1.3 uses similar construction)
        for i in 0..8 {
            iv[4 + i] ^= seq_bytes[i];
        }
        iv
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::packet::{MessageType, Packet};
    use crate::crypto::key_derivation::SessionKeys;
    use bytes::Bytes;
    use uuid::Uuid;

    /// A matched v1 client/server pair derived from the same shared secret.
    fn pair() -> (SecureSession, SecureSession) {
        let secret = [7u8; 32];
        let client = SecureSession::new(Role::Client, SessionKeys::derive(&secret, &[0u8; 32]));
        let server = SecureSession::new(Role::Server, SessionKeys::derive(&secret, &[0u8; 32]));
        (client, server)
    }

    /// A matched **v2** pair (header bound as AAD).
    fn pair_v2() -> (SecureSession, SecureSession) {
        let secret = [7u8; 32];
        let client = SecureSession::new_v2(Role::Client, SessionKeys::derive(&secret, &[0u8; 32]));
        let server = SecureSession::new_v2(Role::Server, SessionKeys::derive(&secret, &[0u8; 32]));
        (client, server)
    }

    /// Encrypt `msg` on the client and wrap it into a wire packet. The header
    /// the session finalized (and, for v2, authenticated) travels with the
    /// packet, exactly as it would on the wire.
    fn seal(client: &mut SecureSession, msg: &[u8]) -> Packet {
        let mut p = Packet::new(MessageType::TextMessage, Bytes::new(), Uuid::nil());
        let (ct, tag) = client.encrypt(msg, &mut p.header).unwrap();
        p.payload = Bytes::from(ct);
        p.auth_tag = Some(tag);
        p
    }

    #[test]
    fn replayed_or_old_sequence_rejected_fresh_accepted() {
        let (mut client, mut server) = pair();

        let p1 = seal(&mut client, b"first"); // seq 1
        let p2 = seal(&mut client, b"second"); // seq 2

        // Fresh, in-order packets are accepted.
        assert_eq!(server.decrypt(&p1).unwrap(), b"first");
        assert_eq!(server.decrypt(&p2).unwrap(), b"second");

        // Replaying an already-accepted (older) sequence is rejected.
        assert!(
            matches!(server.decrypt(&p1), Err(CryptoError::ReplayDetected)),
            "an older/replayed sequence must be dropped"
        );
        // Replaying the most-recently-accepted sequence is likewise rejected.
        assert!(
            matches!(server.decrypt(&p2), Err(CryptoError::ReplayDetected)),
            "a duplicate of the highest-seen sequence must be dropped"
        );

        // A subsequent fresh sequence is still accepted.
        let p3 = seal(&mut client, b"third"); // seq 3
        assert_eq!(server.decrypt(&p3).unwrap(), b"third");
    }

    #[test]
    fn forged_tag_does_not_advance_replay_window() {
        // A failed authentication must NOT move the replay window forward,
        // otherwise an attacker could send a high-sequence forgery to lock out
        // every genuine packet that follows.
        let (mut client, mut server) = pair();
        let good = seal(&mut client, b"hello"); // seq 1

        let mut forged = good.clone();
        let mut tag = forged.auth_tag.unwrap();
        tag[0] ^= 0x01;
        forged.auth_tag = Some(tag);

        assert!(
            server.decrypt(&forged).is_err(),
            "forged tag must fail to decrypt"
        );
        // The genuine seq-1 packet is still accepted afterwards.
        assert_eq!(server.decrypt(&good).unwrap(), b"hello");
    }

    #[test]
    fn v2_binds_header_as_aad_v1_does_not() {
        // v2: an unmodified packet decrypts...
        let (mut c2, mut s2) = pair_v2();
        let p = seal(&mut c2, b"payload"); // seq 1
        assert_eq!(s2.decrypt(&p).unwrap(), b"payload");

        // ...but tampering the msg_type in the header fails the tag, because the
        // header is the AEAD AAD in v2. (The payload/IV are untouched — only the
        // header changed — so this isolates the AAD binding.)
        let mut tampered = seal(&mut c2, b"again"); // seq 2
        tampered.header.msg_type = MessageType::Disconnect;
        assert!(
            s2.decrypt(&tampered).is_err(),
            "v2 must reject a packet whose header was altered in flight"
        );

        // v1 contrast: the SAME tamper still decrypts, because v1 uses empty AAD
        // and does not authenticate the header. This is exactly the gap v2 closes.
        let (mut c1, mut s1) = pair();
        let mut q = seal(&mut c1, b"payload");
        q.header.msg_type = MessageType::Disconnect;
        assert_eq!(
            s1.decrypt(&q).unwrap(),
            b"payload",
            "v1 does not bind the header (documents the pre-v2 gap)"
        );
    }
}
