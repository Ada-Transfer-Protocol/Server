use std::env;
use dotenvy::dotenv;
use log::info;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub auth_driver: String,
    pub auth_file_path: String,
    pub auth_api_url: String,
    pub database_url: String,
}

impl Config {
    pub fn load() -> Self {
        dotenv().ok(); // Load .env file if it exists, ignore if not

        let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8443".to_string())
            .parse()
            .expect("SERVER_PORT must be a number");
        
        let auth_driver = env::var("AUTH_DRIVER").unwrap_or_else(|_| "none".to_string());
        
        let auth_file_path = env::var("AUTH_FILE_PATH").unwrap_or_else(|_| "./users.json".to_string());
        let auth_api_url = env::var("AUTH_API_URL").unwrap_or_else(|_| "http://localhost/login".to_string());
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "".to_string());

        info!("Configuration loaded: Driver={}", auth_driver);

        Self {
            host,
            port,
            auth_driver,
            auth_file_path,
            auth_api_url,
            database_url,
        }
    }
}
