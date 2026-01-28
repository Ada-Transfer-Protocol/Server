use super::{AuthProvider, drivers};
use crate::config::Config;
use std::sync::Arc;

pub fn create_auth_provider(config: &Config) -> Arc<dyn AuthProvider> {
    match config.auth_driver.as_str() {
        "file" => Arc::new(drivers::FileAuthDriver::new(&config.auth_file_path)),
        "api" => Arc::new(drivers::ApiAuthDriver::new(&config.auth_api_url)),
        "postgres" | "mysql" | "sqlite" => Arc::new(drivers::DatabaseAuthDriver::new(&config.database_url)),
        "none" | _ => Arc::new(drivers::NoAuthDriver),
    }
}
