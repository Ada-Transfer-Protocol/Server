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
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
