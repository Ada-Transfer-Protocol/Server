pub mod aes_gcm;
pub mod ed25519;
pub mod key_derivation;
pub mod x25519; // Added

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

    #[error("Replay detected: sequence already seen or older than the highest accepted")]
    ReplayDetected,
}
