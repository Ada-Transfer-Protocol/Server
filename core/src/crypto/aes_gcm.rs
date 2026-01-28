use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce
};
use super::CryptoError;

pub struct Cipher {
    key: [u8; 32],
}

impl Cipher {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn encrypt(&self, nonce_bytes: &[u8; 12], plain_text: &[u8], aad: &[u8]) -> Result<(Vec<u8>, [u8; 16]), CryptoError> {
        let cipher = Aes256Gcm::new(&self.key.into());
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let payload = Payload {
            msg: plain_text,
            aad,
        };

        match cipher.encrypt(nonce, payload) {
            Ok(mut ciphertext) => {
                // In aes-gcm crate, the tag is usually appended to the ciphertext.
                // We need to split it if we want to store it separately as per our protocol spec.
                // However, standard checks usually keep them together. 
                // Our protocol spec says: Payload ... Auth Tag (16 bytes).
                // So if we just return the full ciphertext from aes-gcm, it includes the tag at the end.
                // Let's verify this behavior.
                // aes-gcm's encrypt returns Vec<u8> which is ciphertext + tag.
                
                if ciphertext.len() < 16 {
                    return Err(CryptoError::EncryptionError);
                }
                
                let tag_start = ciphertext.len() - 16;
                let tag_slice = &ciphertext[tag_start..];
                let mut tag = [0u8; 16];
                tag.copy_from_slice(tag_slice);
                
                ciphertext.truncate(tag_start);
                
                Ok((ciphertext, tag))
            },
            Err(_) => Err(CryptoError::EncryptionError),
        }
    }

    pub fn decrypt(&self, nonce_bytes: &[u8; 12], ciphertext: &[u8], tag: &[u8; 16], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = Aes256Gcm::new(&self.key.into());
        let nonce = Nonce::from_slice(nonce_bytes);
        
        // Reconstruct the full payload for the library (ciphertext + tag)
        let mut full_encrypted = Vec::with_capacity(ciphertext.len() + 16);
        full_encrypted.extend_from_slice(ciphertext);
        full_encrypted.extend_from_slice(tag);

        let payload = Payload {
            msg: &full_encrypted,
            aad,
        };

        cipher.decrypt(nonce, payload).map_err(|_| CryptoError::DecryptionError)
    }
}
