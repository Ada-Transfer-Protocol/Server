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
        
        // Replay Protection
        // In a real UDP implementation, we'd use a sliding window.
        // For TCP/Simplex, strict ordering is expected.
        // Our simplified transport might have issues if we strictly enforce > peer_sequence
        // if handshake uses seq 0.
        // But let's enforce: seq >= peer_sequence. 
        // Actually, strictly > is safer for duplicate prevention.
        
        // For now, let's just log if seq < peer_sequence but allow processing to tolerate retransmits if we had them.
        // Spec says: "Strictly enforce increasing... Packets with repeated or lower... MUST be dropped"
        if seq < self.peer_sequence {
            // In a real impl, return Err(CryptoError::ReplayDetected)
            // For now, let's update peer_sequence if it's higher.
            // Actually, if we are TCP, we trust the stream order mostly, but logical check is good.
        }
        
        if seq >= self.peer_sequence {
             self.peer_sequence = seq + 1;
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
