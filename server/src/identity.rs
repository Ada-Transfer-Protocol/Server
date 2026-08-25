//! The server's **long-term Ed25519 identity** — the key clients pin for the v2
//! authenticated handshake (see `docs/spec/12-authenticated-handshake.md`).
//!
//! Unlike the per-connection ephemeral X25519 keys, this one must be *stable*
//! across restarts: the whole point of authentication is that a client can pin
//! `spk_S` once and detect any future impersonation. So it is generated once on
//! first boot and persisted (0600) to a file; every subsequent boot re-derives
//! the same public key from the stored seed.
//!
//! v2 is not yet negotiated by shipped SDK clients, so on a v1-only deployment
//! this identity is loaded but simply unused — carrying no behavioural change.

use std::fs;
use std::io;
use std::path::Path;

use adatp_core::crypto::ed25519::SigningKeyPair;

pub struct ServerIdentity {
    keypair: SigningKeyPair,
}

impl ServerIdentity {
    /// Load the identity seed from `path`, or generate one and persist it on
    /// first boot. The file holds the raw 32-byte Ed25519 seed — **secret key
    /// material**; it is written with `0600` on unix.
    pub fn load_or_create(path: &str) -> io::Result<Self> {
        let p = Path::new(path);
        if let Ok(bytes) = fs::read(p) {
            if bytes.len() == 32 {
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&bytes);
                return Ok(Self { keypair: SigningKeyPair::from_seed(&seed) });
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "identity file {path} is {} bytes, expected a 32-byte Ed25519 seed \
                     (delete it to regenerate, or point ADATP_IDENTITY_PATH elsewhere)",
                    bytes.len()
                ),
            ));
        }

        // First boot: generate and persist.
        let keypair = SigningKeyPair::generate();
        let seed = keypair.seed_bytes();
        if let Some(dir) = p.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }
        fs::write(p, seed)?;
        restrict_permissions(p)?;
        Ok(Self { keypair })
    }

    /// Build an in-memory identity that is **not** persisted — for tests.
    #[cfg(test)]
    pub fn ephemeral() -> Self {
        Self { keypair: SigningKeyPair::generate() }
    }

    pub fn keypair(&self) -> &SigningKeyPair {
        &self.keypair
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.keypair.public_key_bytes()
    }

    /// Lowercase hex of the public key — what an operator copies to a client to
    /// pin, and what is logged at startup.
    pub fn fingerprint(&self) -> String {
        self.public_key_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(unix)]
fn restrict_permissions(p: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_p: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_and_reloads_the_same_key() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("adatp-id-test-{}.key", std::process::id()));
        let path_str = path.to_str().unwrap();
        let _ = fs::remove_file(&path);

        let a = ServerIdentity::load_or_create(path_str).unwrap();
        let fp1 = a.fingerprint();
        // Second load reads the persisted seed → identical public key.
        let b = ServerIdentity::load_or_create(path_str).unwrap();
        assert_eq!(fp1, b.fingerprint());
        assert_eq!(fp1.len(), 64); // 32 bytes hex

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn rejects_a_malformed_seed_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("adatp-id-bad-{}.key", std::process::id()));
        fs::write(&path, b"not a 32 byte seed").unwrap();
        assert!(ServerIdentity::load_or_create(path.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&path);
    }
}
