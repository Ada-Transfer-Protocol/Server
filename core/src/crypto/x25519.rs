use x25519_dalek::{EphemeralSecret, PublicKey};
use rand::rngs::OsRng;
use super::CryptoError;

pub struct KeyPair {
    pub secret: EphemeralSecret,
    pub public: PublicKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }
}

pub fn diffie_hellman(secret: EphemeralSecret, peer_public: &[u8]) -> Result<[u8; 32], CryptoError> {
    if peer_public.len() != 32 {
        return Err(CryptoError::InvalidKey);
    }
    
    let mut arr = [0u8; 32];
    arr.copy_from_slice(peer_public);
    let peer_pk = PublicKey::from(arr);
    
    let shared_secret = secret.diffie_hellman(&peer_pk);
    Ok(*shared_secret.as_bytes())
}
