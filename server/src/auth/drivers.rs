use super::{AuthProvider, AuthError, UserData};
use async_trait::async_trait;
use serde::Deserialize;
use std::fs;
use std::collections::HashMap;

// --- 1. No Auth (Optional Mode) ---
pub struct NoAuthDriver;

#[async_trait]
impl AuthProvider for NoAuthDriver {
    async fn authenticate(&self, username: &str, _password: &str) -> Result<UserData, AuthError> {
        // Always allow, assign 'guest' role
        Ok(UserData {
            username: username.to_string(),
            role: "guest".to_string(),
        })
    }
    
    fn is_auth_required(&self) -> bool { false }
}

// --- 2. File Auth (JSON) ---
#[derive(Deserialize)]
struct JsonUser {
    username: String,
    password: String,
    role: String,
}

pub struct FileAuthDriver {
    users: HashMap<String, JsonUser>,
}

impl FileAuthDriver {
    pub fn new(path: &str) -> Self {
        let content = fs::read_to_string(path).unwrap_or_else(|_| "[]".to_string());
        let user_list: Vec<JsonUser> = serde_json::from_str(&content).unwrap_or_else(|_| Vec::new());
        
        let mut users = HashMap::new();
        for user in user_list {
            users.insert(user.username.clone(), user);
        }
        
        Self { users }
    }
}

#[async_trait]
impl AuthProvider for FileAuthDriver {
    async fn authenticate(&self, username: &str, password: &str) -> Result<UserData, AuthError> {
        if let Some(user) = self.users.get(username) {
            // Note: In production use hashing! (Argon2/bcrypt)
            // Here checking raw password for demo simplicity as per request
            if user.password == password {
                return Ok(UserData {
                    username: user.username.clone(),
                    role: user.role.clone(),
                });
            }
        }
        Err(AuthError::InvalidCredentials)
    }
    fn is_auth_required(&self) -> bool { true }
}

// --- 3. API Auth (JSON POST) ---
pub struct ApiAuthDriver {
    client: reqwest::Client,
    url: String,
}

impl ApiAuthDriver {
    pub fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct ApiAuthResponse {
    success: bool,
    role: Option<String>,
}

#[async_trait]
impl AuthProvider for ApiAuthDriver {
    async fn authenticate(&self, username: &str, password: &str) -> Result<UserData, AuthError> {
        let body = serde_json::json!({
            "username": username,
            "password": password
        });

        match self.client.post(&self.url).json(&body).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let auth_data: ApiAuthResponse = resp.json().await.map_err(|e| AuthError::InternalError(e.to_string()))?;
                    if auth_data.success {
                        return Ok(UserData {
                            username: username.to_string(),
                            role: auth_data.role.unwrap_or("user".to_string()),
                        });
                    }
                }
                Err(AuthError::InvalidCredentials)
            },
            Err(e) => Err(AuthError::InternalError(e.to_string())),
        }
    }
    fn is_auth_required(&self) -> bool { true }
}

// --- 4. Database Drivers (Skeleton) ---
// For comprehensive support, logic would be added here using `sqlx`.
// Since we didn't enable heavy features to save time, this is a placeholder structure.
pub struct DatabaseAuthDriver {
    // pool: sqlx::Pool<...>,
}

impl DatabaseAuthDriver {
    pub fn new(_url: &str) -> Self {
        Self {}
    }
}

#[async_trait]
impl AuthProvider for DatabaseAuthDriver {
    async fn authenticate(&self, _username: &str, _password: &str) -> Result<UserData, AuthError> {
        Err(AuthError::InternalError("Database driver not fully compiled yet.".to_string()))
    }
    fn is_auth_required(&self) -> bool { true }
}
