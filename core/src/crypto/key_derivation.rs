use hkdf::Hkdf;
use sha2::Sha256;


pub struct SessionKeys {
    pub client_write_key: [u8; 32],
    pub server_write_key: [u8; 32],
    pub client_iv_root: [u8; 12],
    pub server_iv_root: [u8; 12],
}

impl SessionKeys {
    pub fn derive(shared_secret: &[u8], salt: &[u8]) -> Self {
        let hkdf = Hkdf::<Sha256>::new(Some(salt), shared_secret);
        
        let mut client_write_key = [0u8; 32];
        let mut server_write_key = [0u8; 32];
        let mut client_iv_root = [0u8; 12];
        let mut server_iv_root = [0u8; 12];

        // We ignore errors here because lengths are hardcoded and correct.
        // In a rigorous implementation, we should handle them, but panic is unlikely given fixed sizes.
        hkdf.expand(b"client_write", &mut client_write_key).unwrap();
        hkdf.expand(b"server_write", &mut server_write_key).unwrap();
        hkdf.expand(b"client_iv", &mut client_iv_root).unwrap();
        hkdf.expand(b"server_iv", &mut server_iv_root).unwrap();

        Self {
            client_write_key,
            server_write_key,
            client_iv_root,
            server_iv_root,
        }
    }
}
