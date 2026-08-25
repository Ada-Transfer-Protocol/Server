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

/// A configured webhook endpoint.
#[derive(Serialize, sqlx::FromRow, Clone, Debug)]
pub struct WebhookRow {
    pub id: String,
    pub url: String,
    /// HMAC-SHA256 signing secret. Redacted in admin listings.
    pub secret: String,
    /// JSON array of event filters (exact names, `prefix.*`, or `*`).
    pub events: String,
    pub is_active: bool,
    pub description: Option<String>,
    pub created_at: String,
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

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS webhooks (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                secret TEXT NOT NULL,
                events TEXT NOT NULL DEFAULT '["*"]',
                is_active BOOLEAN DEFAULT TRUE,
                description TEXT,
                created_at TEXT NOT NULL
            );
            "#,
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

    /// Cheap connectivity probe used by /readyz.
    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Webhook endpoints
    // ------------------------------------------------------------------

    pub async fn webhook_list(&self) -> Result<Vec<WebhookRow>> {
        let rows = sqlx::query_as::<_, WebhookRow>(
            "SELECT * FROM webhooks ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn webhook_create(
        &self,
        url: &str,
        secret: &str,
        events: &[String],
        description: Option<&str>,
    ) -> Result<WebhookRow> {
        let row = WebhookRow {
            id: Uuid::new_v4().to_string(),
            url: url.to_string(),
            secret: secret.to_string(),
            events: serde_json::to_string(events)?,
            is_active: true,
            description: description.map(|s| s.to_string()),
            created_at: Utc::now().to_rfc3339(),
        };
        sqlx::query(
            "INSERT INTO webhooks (id, url, secret, events, is_active, description, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.url)
        .bind(&row.secret)
        .bind(&row.events)
        .bind(row.is_active)
        .bind(&row.description)
        .bind(&row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn webhook_set_active(&self, id: &str, active: bool) -> Result<bool> {
        let res = sqlx::query("UPDATE webhooks SET is_active = ? WHERE id = ?")
            .bind(active)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn webhook_delete(&self, id: &str) -> Result<bool> {
        let res = sqlx::query("DELETE FROM webhooks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
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
