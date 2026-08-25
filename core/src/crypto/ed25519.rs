use super::CryptoError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::{rngs::OsRng, RngCore};

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

    /// Reconstruct a key pair from a persisted 32-byte Ed25519 seed. This is
    /// how a server's *long-term* identity survives a restart: the seed is
    /// stored once (see the server's identity file) and the same public key —
    /// the one clients pin — is derived from it every boot. Contrast with
    /// [`generate`], which is for throwaway/ephemeral identities in tests.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            keypair: SigningKey::from_bytes(seed),
        }
    }

    /// The 32-byte seed, for persisting a long-term identity. Treat it as
    /// **secret key material** — anyone holding it can impersonate this server.
    pub fn seed_bytes(&self) -> [u8; 32] {
        self.keypair.to_bytes()
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

    verifier
        .verify(message, &sig)
        .map_err(|_| CryptoError::SignatureError)
}
