use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    pub username: String,
    pub role: String,
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    #[allow(dead_code)]
    InternalError(String),
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<UserData, AuthError>;
    fn is_auth_required(&self) -> bool;
}

pub mod drivers;
pub mod factory;
