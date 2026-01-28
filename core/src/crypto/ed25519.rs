use ed25519_dalek::{Signer, Verifier, Signature, SigningKey, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use super::CryptoError;

pub struct SigningKeyPair {
    keypair: SigningKey,
}

impl SigningKeyPair {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let mut bytes = [0u8; 32];
        csprng.fill_bytes(&mut bytes);
        let keypair = SigningKey::from_bytes(&bytes);
        Self { keypair }
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.keypair.sign(message).to_bytes()
    }
    
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.keypair.verifying_key().to_bytes()
    }
}

pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
    if public_key.len() != 32 || signature.len() != 64 {
        return Err(CryptoError::InvalidKey);
    }

    let mut pk_bytes = [0u8; 32];
    pk_bytes.copy_from_slice(public_key);
    
    let verifier = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| CryptoError::InvalidKey)?;
    
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(signature);
    let sig = Signature::from_bytes(&sig_bytes);

    verifier.verify(message, &sig).map_err(|_| CryptoError::SignatureError)
}
