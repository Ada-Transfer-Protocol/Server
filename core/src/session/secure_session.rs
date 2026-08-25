use crate::crypto::{aes_gcm::Cipher, key_derivation::SessionKeys};
use crate::codec::packet::{Packet, PacketFlags};
use crate::crypto::CryptoError;

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
}

impl SecureSession {
    pub fn new(role: Role, keys: SessionKeys) -> Self {
        let cipher_client = Cipher::new(keys.client_write_key);
        let cipher_server = Cipher::new(keys.server_write_key);

        Self {
            role,
            keys,
            cipher_client,
            cipher_server,
            my_sequence: 1, // Start from 1, 0 might be used for handshake packets if unencrypted
            peer_sequence: 1,
        }
    }

    // Encrypts a payload and prepares the packet parameters (like IV generation)
    // Returns (EncryptedPayload, AuthTag, SequenceUsed)
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 16], u64), CryptoError> {
        let seq = self.my_sequence;
        // IV = IV_Root XOR Sequence (8 bytes + padding? Or just XOR last 8 bytes?)
        // Spec says: IV = IV_Root XOR Sequence
        // IV Root is 12 bytes. Sequence is 8 bytes.
        // Let's XOR the last 8 bytes of IV Root with Sequence.
        
        let iv = self.compute_iv(seq, &self.role);
        
        let (ciphertext, tag) = match self.role {
            Role::Client => self.cipher_client.encrypt(&iv, plaintext, &[])?,
            Role::Server => self.cipher_server.encrypt(&iv, plaintext, &[])?,
        };
        
        self.my_sequence += 1;
        Ok((ciphertext, tag, seq))
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

        let plaintext = match self.role {
            Role::Client => self.cipher_server.decrypt(&iv, &packet.payload, &tag, &[])?,
            Role::Server => self.cipher_client.decrypt(&iv, &packet.payload, &tag, &[])?,
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
    use crate::codec::packet::{MessageType, Packet, PacketFlags};
    use crate::crypto::key_derivation::SessionKeys;
    use bytes::Bytes;
    use uuid::Uuid;

    /// A matched client/server pair derived from the same shared secret.
    fn pair() -> (SecureSession, SecureSession) {
        let secret = [7u8; 32];
        let client = SecureSession::new(Role::Client, SessionKeys::derive(&secret, &[0u8; 32]));
        let server = SecureSession::new(Role::Server, SessionKeys::derive(&secret, &[0u8; 32]));
        (client, server)
    }

    /// Encrypt `msg` on the client and wrap it into a wire packet.
    fn seal(client: &mut SecureSession, msg: &[u8]) -> Packet {
        let (ct, tag, seq) = client.encrypt(msg).unwrap();
        let mut p = Packet::new(MessageType::TextMessage, Bytes::from(ct), Uuid::nil());
        p.header.flags |= PacketFlags::ENCRYPTED;
        p.header.sequence = seq;
        p.header.length = p.payload.len() as u32;
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

        assert!(server.decrypt(&forged).is_err(), "forged tag must fail to decrypt");
        // The genuine seq-1 packet is still accepted afterwards.
        assert_eq!(server.decrypt(&good).unwrap(), b"hello");
    }
}
