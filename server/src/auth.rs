use std::collections::HashMap;
use std::sync::Arc;

use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::{AuthDriver, Config};

/// The identity attached to a connection after successful authentication.
#[derive(Clone, Debug, Serialize)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug)]
pub enum AuthError {
    /// Wrong username/password (or upstream said "no").
    InvalidCredentials,
    /// The verification backend itself failed. Connections are rejected
    /// (fail-closed), never silently admitted.
    Unavailable(String),
}

/// Payload of an `AuthRequest` packet (JSON).
#[derive(Deserialize)]
pub struct AuthRequestBody {
    pub username: String,
    pub password: String,
}

/// One entry of the `users.json` file driver.
///
/// Plaintext passwords are for development and demos only — production
/// deployments should use `AUTH_DRIVER=api`. This is documented in
/// docs/spec/08-security.md.
#[derive(Deserialize, Clone)]
struct FileUser {
    username: String,
    password: String,
    #[serde(default = "default_role")]
    role: String,
}

fn default_role() -> String {
    "user".to_string()
}

/// Response contract of the external `api` driver:
/// `POST AUTH_API_URL { "username": ..., "password": ... }`
/// → `{ "authorized": bool, "user_id": "...", "role": "..." }`
#[derive(Deserialize)]
struct ApiAuthResponse {
    authorized: bool,
    user_id: Option<String>,
    role: Option<String>,
    #[allow(dead_code)]
    error: Option<String>,
}

pub struct AuthManager {
    driver: AuthDriver,
    users: RwLock<HashMap<String, FileUser>>,
    file_path: String,
    api_url: Option<String>,
    http: reqwest::Client,
}

/// Constant-time byte comparison (length differences still leak, which is
/// acceptable for the dev-only file driver).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl AuthManager {
    /// Construct the auth manager for the configured driver.
    ///
    /// The `file` driver is **fail-closed at startup**: if no user file can be
    /// read, or the file defines zero users, the server refuses to start
    /// (returns `Err`) rather than silently coming up with no valid logins or
    /// with left-over demo credentials. Operators who genuinely want an open
    /// server must select `AUTH_DRIVER=none` explicitly. The `api` and `none`
    /// drivers are unaffected.
    pub fn new(cfg: &Config) -> Result<Arc<Self>, String> {
        let mgr = Arc::new(Self {
            driver: cfg.auth_driver.clone(),
            users: RwLock::new(HashMap::new()),
            file_path: cfg.auth_file_path.clone(),
            api_url: cfg.auth_api_url.clone(),
            http: reqwest::Client::new(),
        });

        if mgr.driver == AuthDriver::File {
            match mgr.load_users_blocking() {
                Ok(0) => {
                    return Err(format!(
                        "AUTH_DRIVER=file but the user file '{}' defines zero users. \
                         Refusing to start (fail-closed). Copy server/users.example.json \
                         to '{}' and set real credentials, or choose AUTH_DRIVER=api / \
                         AUTH_DRIVER=none explicitly.",
                        mgr.file_path, mgr.file_path
                    ));
                }
                Ok(n) => info!("Auth driver 'file': {} user(s) loaded", n),
                Err(e) => {
                    return Err(format!(
                        "AUTH_DRIVER=file (default) but no readable user file was found: {e}. \
                         Refusing to start (fail-closed) so the server never runs with demo \
                         or absent credentials. Copy server/users.example.json to '{}', or \
                         set AUTH_DRIVER=api / AUTH_DRIVER=none explicitly.",
                        mgr.file_path
                    ));
                }
            }
        } else {
            info!("Auth driver: {:?}", mgr.driver);
        }

        Ok(mgr)
    }

    /// Loads (or reloads) the user file. Tries `AUTH_FILE_PATH` first, then
    /// `server/<AUTH_FILE_PATH>` so `cargo run` from the workspace root works.
    fn load_users_blocking(&self) -> Result<usize, String> {
        let candidates = [self.file_path.clone(), format!("server/{}", self.file_path)];
        let content = candidates
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .ok_or_else(|| format!("no user file found at {:?}", candidates))?;

        let list: Vec<FileUser> =
            serde_json::from_str(&content).map_err(|e| format!("invalid JSON: {e}"))?;
        let mut map = HashMap::new();
        for u in list {
            map.insert(u.username.clone(), u);
        }
        let n = map.len();
        // No contention at construction time; try_write avoids the
        // panic blocking_write() would raise inside the async runtime.
        *self
            .users
            .try_write()
            .map_err(|_| "user map locked".to_string())? = map;
        Ok(n)
    }

    /// Reload the file driver's user list (used by the admin plane).
    #[allow(dead_code)] // consumed by /admin/v1 (wired in the admin control plane)
    pub async fn reload(&self) -> Result<usize, String> {
        if self.driver != AuthDriver::File {
            return Err("reload only applies to the file driver".into());
        }
        let candidates = [self.file_path.clone(), format!("server/{}", self.file_path)];
        let content = candidates
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .ok_or_else(|| format!("no user file found at {:?}", candidates))?;
        let list: Vec<FileUser> =
            serde_json::from_str(&content).map_err(|e| format!("invalid JSON: {e}"))?;
        let mut map = HashMap::new();
        for u in list {
            map.insert(u.username.clone(), u);
        }
        let n = map.len();
        *self.users.write().await = map;
        Ok(n)
    }

    pub fn driver_name(&self) -> &'static str {
        match self.driver {
            AuthDriver::File => "file",
            AuthDriver::Api => "api",
            AuthDriver::None => "none",
        }
    }

    /// Verify credentials against the configured driver.
    pub async fn verify(&self, username: &str, password: &str) -> Result<AuthUser, AuthError> {
        if username.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }
        match self.driver {
            AuthDriver::None => Ok(AuthUser {
                user_id: username.to_string(),
                username: username.to_string(),
                role: "anonymous".to_string(),
            }),
            AuthDriver::File => {
                let users = self.users.read().await;
                match users.get(username) {
                    Some(u) if ct_eq(u.password.as_bytes(), password.as_bytes()) => Ok(AuthUser {
                        user_id: u.username.clone(),
                        username: u.username.clone(),
                        role: u.role.clone(),
                    }),
                    _ => Err(AuthError::InvalidCredentials),
                }
            }
            AuthDriver::Api => {
                let url = self
                    .api_url
                    .as_ref()
                    .ok_or_else(|| AuthError::Unavailable("AUTH_API_URL unset".into()))?;
                let resp = self
                    .http
                    .post(url)
                    .json(&serde_json::json!({ "username": username, "password": password }))
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                    .map_err(|e| AuthError::Unavailable(e.to_string()))?;

                if !resp.status().is_success() {
                    return Err(AuthError::InvalidCredentials);
                }
                let body: ApiAuthResponse = resp
                    .json()
                    .await
                    .map_err(|e| AuthError::Unavailable(format!("bad auth response: {e}")))?;
                if body.authorized {
                    Ok(AuthUser {
                        user_id: body.user_id.unwrap_or_else(|| username.to_string()),
                        username: username.to_string(),
                        role: body.role.unwrap_or_else(|| "user".to_string()),
                    })
                } else {
                    Err(AuthError::InvalidCredentials)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthDriver;

    fn cfg_for(path: &str, driver: AuthDriver) -> Config {
        Config {
            host: "0.0.0.0".into(),
            port: 3000,
            auth_driver: driver,
            auth_file_path: path.into(),
            auth_api_url: None,
            database_url: "sqlite:adatp.db".into(),
            max_frame_bytes: 1048576,
            idle_timeout_secs: 90,
            plugins_dir: "plugins".into(),
            max_connections: 10000,
            msg_rate_limit: 200,
            room_allowlist: Vec::new(),
            room_protected_prefix: None,
            room_protected_role: "admin".into(),
            identity_path: "adatp-identity.key".into(),
            min_protocol_version: 1,
            backplane_url: None,
        }
    }

    fn write_temp_users(contents: &str) -> String {
        let mut path = std::env::temp_dir();
        let unique = format!(
            "adatp-users-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        path.push(unique);
        std::fs::write(&path, contents).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn file_driver_happy_path_auth_then_join() {
        let path = write_temp_users(r#"[{"username":"alice","password":"s3cret","role":"user"}]"#);
        let cfg = cfg_for(&path, AuthDriver::File);
        let auth = AuthManager::new(&cfg).expect("file driver loads a non-empty user file");

        // Correct credentials authenticate and carry the declared role.
        let user = auth.verify("alice", "s3cret").await.expect("valid login");
        assert_eq!(user.username, "alice");
        assert_eq!(user.role, "user");

        // The authenticated user may join a public room (default permissive policy).
        assert!(cfg.room_join_allowed("global", &user.role).is_ok());

        // Wrong password and unknown user are both rejected.
        assert!(matches!(
            auth.verify("alice", "nope").await,
            Err(AuthError::InvalidCredentials)
        ));
        assert!(matches!(
            auth.verify("mallory", "whatever").await,
            Err(AuthError::InvalidCredentials)
        ));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_driver_refuses_to_start_without_users() {
        // Missing file → fail-closed (Finding 5).
        let cfg = cfg_for("adatp-nonexistent-user-file.json", AuthDriver::File);
        assert!(
            AuthManager::new(&cfg).is_err(),
            "a missing user file must refuse startup, not run open"
        );

        // A file that defines zero users → also fail-closed.
        let path = write_temp_users("[]");
        let cfg = cfg_for(&path, AuthDriver::File);
        assert!(
            AuthManager::new(&cfg).is_err(),
            "zero users must refuse startup"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn none_driver_still_admits_anonymous() {
        // Finding 5 must not weaken the `none` driver.
        let cfg = cfg_for("unused.json", AuthDriver::None);
        let auth = AuthManager::new(&cfg).expect("none driver never touches a file");
        let user = auth.verify("guest", "").await.expect("anonymous accepted");
        assert_eq!(user.role, "anonymous");
    }
}
