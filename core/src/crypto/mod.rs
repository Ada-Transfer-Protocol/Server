pub mod x25519;
pub mod aes_gcm;
pub mod ed25519;
pub mod key_derivation; // Added

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Encryption failed")]
    EncryptionError,
    
    #[error("Decryption failed")]
    DecryptionError,
    
    #[error("Invalid key")]
    InvalidKey,

    #[error("Signature verification failed")]
    SignatureError,
}
