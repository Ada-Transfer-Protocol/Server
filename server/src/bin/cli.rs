use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::Value;
// use std::env; // unused warning
use sqlx::sqlite::SqlitePoolOptions;

#[derive(Parser)]
#[command(name = "adatp-cli")]
#[command(about = "AdaTP Server Management CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// View real-time server statistics (Requires running server)
    Stats {
        #[arg(short, long, default_value = "http://127.0.0.1:3000")]
        url: String,
        #[arg(short, long, env = "ADATP_API_KEY")]
        key: String,
    },
    /// Manage API Keys (Direct Database Access)
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
        #[arg(long, default_value = "sqlite:adatp.db")]
        db_url: String,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Create a new API Key
    Create {
        #[arg(short, long)]
        description: String,
    },
    /// List all API Keys
    List,
    /// Revoke (Deactivate) an API Key
    Revoke { id: String },
    /// Toggle Active Status
    Toggle { id: String, status: bool },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Stats { url, key } => {
            get_stats(&url, &key).await;
        }
        Commands::Auth { action, db_url } => {
            handle_auth(action, &db_url).await?;
        }
    }

    Ok(())
}

async fn get_stats(base_url: &str, key: &str) {
    let client = Client::new();
    let resp = client
        .get(format!("{}/api/metrics", base_url))
        .header("x-api-key", key)
        .send()
        .await;

    match resp {
        Ok(r) => {
            if r.status().is_success() {
                let json: Value = r.json().await.unwrap_or(serde_json::json!({}));
                println!("--- AdaTP Server Metrics ---");
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            } else {
                eprintln!("Failed: Status {}", r.status());
                let text = r.text().await.unwrap_or_default();
                eprintln!("Body: {}", text);
            }
        }
        Err(e) => eprintln!("Connection Error: {}", e),
    }
}

async fn handle_auth(cmd: AuthCommands, db_url: &str) -> anyhow::Result<()> {
    // If db_url is just "sqlite:adatp.db", sqlx might fail if not resolved absolute path or if file doesn't exist?
    // But let's assume it works or user provides full path.

    let pool = SqlitePoolOptions::new()
        .connect(db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Database Connection Failed: {}. Make sure you are in 'server' directory or provide --db-url", e))?;

    match cmd {
        AuthCommands::List => {
            use sqlx::Row;
            let rows =
                sqlx::query("SELECT id, key, description, is_active, created_at FROM api_keys")
                    .fetch_all(&pool)
                    .await?;

            println!(
                "{:<36} | {:<32} | {:<20} | {:<6} | {}",
                "ID", "Key", "Desc", "Active", "Created"
            );
            println!("{}", "-".repeat(120));
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let key: String = row.try_get("key").unwrap_or_default();
                let description: String = row.try_get("description").unwrap_or_default();
                let is_active: bool = row.try_get("is_active").unwrap_or(false);
                let created_at: String = row.try_get("created_at").unwrap_or_default();
                println!(
                    "{:<36} | {:<32} | {:<20} | {:<6} | {}",
                    id, key, description, is_active, created_at
                );
            }
        }
        AuthCommands::Create { description } => {
            let id = uuid::Uuid::new_v4().to_string();
            let key = uuid::Uuid::new_v4().to_string().replace("-", "");
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query("INSERT INTO api_keys (id, key, description, is_active, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(&key)
                .bind(&description)
                .bind(true)
                .bind(&now)
                .execute(&pool)
                .await?;

            println!("Created API Key:");
            println!("ID: {}", id);
            println!("Key: {}", key);
        }
        AuthCommands::Revoke { id } => {
            sqlx::query("UPDATE api_keys SET is_active = FALSE WHERE id = ?")
                .bind(&id)
                .execute(&pool)
                .await?;
            println!("Revoked key {}", id);
        }
        AuthCommands::Toggle { id, status } => {
            sqlx::query("UPDATE api_keys SET is_active = ? WHERE id = ?")
                .bind(status)
                .bind(&id)
                .execute(&pool)
                .await?;
            println!("Set key {} status to {}", id, status);
        }
    }

    Ok(())
}
