use super::CryptoError;
use rand::rngs::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey};

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

pub fn diffie_hellman(
    secret: EphemeralSecret,
    peer_public: &[u8],
) -> Result<[u8; 32], CryptoError> {
    if peer_public.len() != 32 {
        return Err(CryptoError::InvalidKey);
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(peer_public);
    let peer_pk = PublicKey::from(arr);

    let shared_secret = secret.diffie_hellman(&peer_pk);
    // Reject a non-contributory exchange: if the peer sent a low-order point the
    // shared secret is all-zero (a degenerate key an active attacker could force).
    // x25519-dalek does not reject these by default, so we check explicitly.
    if !shared_secret.was_contributory() {
        return Err(CryptoError::InvalidKey);
    }
    Ok(*shared_secret.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_low_order_point() {
        // 32 zero bytes is a low-order point: the DH is non-contributory (the
        // shared secret is all-zero), so it must be rejected.
        let kp = KeyPair::generate();
        assert!(matches!(
            diffie_hellman(kp.secret, &[0u8; 32]),
            Err(CryptoError::InvalidKey)
        ));
    }

    #[test]
    fn accepts_normal_exchange() {
        let a = KeyPair::generate();
        let b = KeyPair::generate();
        let b_pub = *b.public.as_bytes();
        assert!(diffie_hellman(a.secret, &b_pub).is_ok());
    }

    #[test]
    fn rejects_wrong_length_key() {
        let kp = KeyPair::generate();
        assert!(matches!(
            diffie_hellman(kp.secret, &[0u8; 31]),
            Err(CryptoError::InvalidKey)
        ));
    }
}
