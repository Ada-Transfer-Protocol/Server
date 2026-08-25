use std::env;

/// Authentication driver selection.
///
/// - `File`: verify credentials against a local JSON user file (default).
/// - `Api`:  delegate verification to an external HTTP endpoint (`AUTH_API_URL`).
/// - `None`: anonymous mode — every AuthRequest is accepted and tagged with
///   the `anonymous` role. Intended for development / public-lobby setups only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthDriver {
    File,
    Api,
    None,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Bind address. `HOST` (fallback `SERVER_HOST`), default `0.0.0.0`.
    pub host: String,
    /// Bind port. `PORT` (fallback `SERVER_PORT`), default `3000`.
    pub port: u16,
    pub auth_driver: AuthDriver,
    /// Path of the JSON user file for the `file` driver. `AUTH_FILE_PATH`.
    pub auth_file_path: String,
    /// External verification endpoint for the `api` driver. `AUTH_API_URL`.
    pub auth_api_url: Option<String>,
    /// SQLite connection string for API keys / persistence. `DATABASE_URL`.
    pub database_url: String,
    /// Maximum accepted AdaTP payload size in bytes. `MAX_FRAME_BYTES`, default 1 MiB.
    pub max_frame_bytes: usize,
    /// Seconds of full silence after which a connection is dropped. `IDLE_TIMEOUT_SECS`, default 90.
    pub idle_timeout_secs: u64,
    /// Directory scanned for plugins. `PLUGINS_DIR`, default `plugins`.
    pub plugins_dir: String,
    /// Hard cap on concurrent WebSocket connections. `MAX_CONNECTIONS`, default 10000.
    /// New connections above the cap are rejected at upgrade with HTTP 503.
    pub max_connections: usize,
    /// Per-connection inbound message rate limit in messages/second.
    /// `MSG_RATE_LIMIT`, default 200. `0` disables the limit. Connections that
    /// exceed it are closed (`rate_limited`).
    pub msg_rate_limit: u32,
    /// Optional room allowlist. `ROOM_ALLOWLIST` (comma-separated). When
    /// non-empty, only listed rooms may be joined. Empty = all rooms (default).
    pub room_allowlist: Vec<String>,
    /// Rooms whose name starts with this prefix require `room_protected_role`.
    /// `ROOM_PROTECTED_PREFIX`, default unset (no prefix is protected).
    pub room_protected_prefix: Option<String>,
    /// Role required to join `room_protected_prefix` rooms. `ROOM_PROTECTED_ROLE`,
    /// default `admin`.
    pub room_protected_role: String,
}

impl Config {
    pub fn load() -> Self {
        let host = env::var("HOST")
            .or_else(|_| env::var("SERVER_HOST"))
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let port: u16 = env::var("PORT")
            .or_else(|_| env::var("SERVER_PORT"))
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .expect("PORT must be a number");

        let auth_api_url = env::var("AUTH_API_URL").ok().filter(|s| !s.is_empty());

        let auth_driver = match env::var("AUTH_DRIVER").as_deref() {
            Ok("file") => AuthDriver::File,
            Ok("api") => AuthDriver::Api,
            Ok("none") => AuthDriver::None,
            Ok(other) => panic!("Unknown AUTH_DRIVER '{other}' (expected file|api|none)"),
            // Back-compat: setting AUTH_API_URL alone selects the api driver.
            Err(_) if auth_api_url.is_some() => AuthDriver::Api,
            Err(_) => AuthDriver::File,
        };

        if auth_driver == AuthDriver::Api && auth_api_url.is_none() {
            panic!("AUTH_DRIVER=api requires AUTH_API_URL to be set");
        }

        let auth_file_path =
            env::var("AUTH_FILE_PATH").unwrap_or_else(|_| "users.json".to_string());

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:adatp.db".to_string());

        let max_frame_bytes: usize = env::var("MAX_FRAME_BYTES")
            .unwrap_or_else(|_| "1048576".to_string())
            .parse()
            .expect("MAX_FRAME_BYTES must be a number");

        let idle_timeout_secs: u64 = env::var("IDLE_TIMEOUT_SECS")
            .unwrap_or_else(|_| "90".to_string())
            .parse()
            .expect("IDLE_TIMEOUT_SECS must be a number");

        // Like users.json, fall back to server/plugins so `cargo run` from
        // the workspace root finds the bundled example plugins.
        let plugins_dir = env::var("PLUGINS_DIR").unwrap_or_else(|_| {
            if std::path::Path::new("plugins").is_dir() {
                "plugins".to_string()
            } else if std::path::Path::new("server/plugins").is_dir() {
                "server/plugins".to_string()
            } else {
                "plugins".to_string()
            }
        });

        let max_connections: usize = env::var("MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10000".to_string())
            .parse()
            .expect("MAX_CONNECTIONS must be a number");

        let msg_rate_limit: u32 = env::var("MSG_RATE_LIMIT")
            .unwrap_or_else(|_| "200".to_string())
            .parse()
            .expect("MSG_RATE_LIMIT must be a number");

        let room_allowlist: Vec<String> = env::var("ROOM_ALLOWLIST")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let room_protected_prefix = env::var("ROOM_PROTECTED_PREFIX")
            .ok()
            .filter(|s| !s.is_empty());

        let room_protected_role =
            env::var("ROOM_PROTECTED_ROLE").unwrap_or_else(|_| "admin".to_string());

        Self {
            host,
            port,
            auth_driver,
            auth_file_path,
            auth_api_url,
            database_url,
            max_frame_bytes,
            idle_timeout_secs,
            plugins_dir,
            max_connections,
            msg_rate_limit,
            room_allowlist,
            room_protected_prefix,
            room_protected_role,
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Built-in room-join policy, enforced before a join takes effect and
    /// before the plugin `join` veto hook. Returns `Ok(())` when the role may
    /// join `room`, or `Err(reason)` with a stable machine-readable reason.
    ///
    /// Default configuration is permissive (public rooms): an empty allowlist
    /// and no protected prefix admit every room, so nothing breaks unless an
    /// operator opts in via `ROOM_ALLOWLIST` / `ROOM_PROTECTED_PREFIX`.
    pub fn room_join_allowed(&self, room: &str, role: &str) -> Result<(), &'static str> {
        if !self.room_allowlist.is_empty()
            && !self.room_allowlist.iter().any(|r| r == room)
        {
            return Err("room_not_allowed");
        }
        if let Some(prefix) = self.room_protected_prefix.as_deref() {
            if room.starts_with(prefix) && role != self.room_protected_role {
                return Err("room_forbidden");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Config {
        Config {
            host: "0.0.0.0".into(),
            port: 3000,
            auth_driver: AuthDriver::None,
            auth_file_path: "users.json".into(),
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
        }
    }

    #[test]
    fn default_policy_is_permissive() {
        let cfg = base();
        assert!(cfg.room_join_allowed("global", "user").is_ok());
        assert!(cfg.room_join_allowed("anything-goes", "anonymous").is_ok());
    }

    #[test]
    fn allowlist_blocks_unlisted_rooms() {
        let mut cfg = base();
        cfg.room_allowlist = vec!["lobby".into(), "global".into()];
        assert!(cfg.room_join_allowed("lobby", "user").is_ok());
        assert!(cfg.room_join_allowed("global", "user").is_ok());
        assert_eq!(cfg.room_join_allowed("secret", "user"), Err("room_not_allowed"));
    }

    #[test]
    fn protected_prefix_requires_role() {
        let mut cfg = base();
        cfg.room_protected_prefix = Some("admin-".into());
        cfg.room_protected_role = "admin".into();
        // Non-admin cannot join a protected room.
        assert_eq!(cfg.room_join_allowed("admin-ops", "user"), Err("room_forbidden"));
        // The right role can.
        assert!(cfg.room_join_allowed("admin-ops", "admin").is_ok());
        // Unprotected rooms are unaffected by the prefix rule.
        assert!(cfg.room_join_allowed("general", "user").is_ok());
    }

    #[test]
    fn allowlist_and_prefix_compose() {
        let mut cfg = base();
        cfg.room_allowlist = vec!["admin-ops".into(), "general".into()];
        cfg.room_protected_prefix = Some("admin-".into());
        // In the allowlist but wrong role → still forbidden by the prefix rule.
        assert_eq!(cfg.room_join_allowed("admin-ops", "user"), Err("room_forbidden"));
        // In the allowlist and right role → allowed.
        assert!(cfg.room_join_allowed("admin-ops", "admin").is_ok());
        // Not in the allowlist → rejected before the prefix rule is considered.
        assert_eq!(cfg.room_join_allowed("random", "admin"), Err("room_not_allowed"));
    }
}
