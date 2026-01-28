use sqlx::SqlitePool;
use anyhow::Result;
use uuid::Uuid;
use serde::Serialize;
use chrono::Utc;

#[derive(Serialize, sqlx::FromRow)]
#[allow(dead_code)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: String, // Sqlite text
}

#[allow(dead_code)]
pub struct DbManager {
    pool: SqlitePool,
}

impl DbManager {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        
        // Init Schema
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                key TEXT UNIQUE NOT NULL,
                description TEXT,
                is_active BOOLEAN DEFAULT TRUE,
                created_at TEXT NOT NULL
            );
            "#
        )
        .execute(&pool)
        .await?;

        // Create default admin key if none exists
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
            .fetch_one(&pool)
            .await?;
        
        if count == 0 {
            let id = Uuid::new_v4().to_string();
            let key = "admin-secret-key".to_string(); // In production, this should be random
            let now = Utc::now().to_rfc3339();
            
            sqlx::query("INSERT INTO api_keys (id, key, description, is_active, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(id)
                .bind(key)
                .bind("Default Admin Key")
                .bind(true)
                .bind(now)
                .execute(&pool)
                .await?;
            
            log::info!("Created default admin API Key: admin-secret-key");
        }

        Ok(Self { pool })
    }

    #[allow(dead_code)]
    pub async fn create_key(&self, description: &str) -> Result<ApiKey> {
        let id = Uuid::new_v4().to_string();
        let key = Uuid::new_v4().to_string().replace("-", ""); // Simple random key
        let now = Utc::now().to_rfc3339();
        
        sqlx::query("INSERT INTO api_keys (id, key, description, is_active, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&key)
            .bind(description)
            .bind(true)
            .bind(&now)
            .execute(&self.pool)
            .await?;
            
        Ok(ApiKey {
            id,
            key,
            description: Some(description.to_string()),
            is_active: true,
            created_at: now,
        })
    }

    #[allow(dead_code)]
    pub async fn list_keys(&self) -> Result<Vec<ApiKey>> {
        let keys = sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(keys)
    }

    pub async fn validate_key(&self, key: &str) -> Result<bool> {
        let result: Option<bool> = sqlx::query_scalar("SELECT is_active FROM api_keys WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
            
        Ok(result.unwrap_or(false))
    }
    
    #[allow(dead_code)]
    pub async fn revoke_key(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE api_keys SET is_active = FALSE WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    
    #[allow(dead_code)]
    pub async fn toggle_key(&self, id: &str, active: bool) -> Result<()> {
        sqlx::query("UPDATE api_keys SET is_active = ? WHERE id = ?")
            .bind(active)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
