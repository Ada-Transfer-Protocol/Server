use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use dotenvy::dotenv;
use log::info;

mod admin;
mod api;
mod auth;
mod config;
mod connection;
mod db;
mod hub;
mod identity;
mod load;
mod logging;
mod metrics;
mod plugins;
mod silo;
mod webhooks;

use crate::api::AppState;
use crate::auth::AuthManager;
use crate::config::Config;
use crate::db::DbManager;
use crate::hub::Hub;
use crate::metrics::Metrics;
use crate::plugins::PluginManager;
use crate::webhooks::WebhookManager;

/// AdaTP v1 server.
///
/// Single listener (default 0.0.0.0:3000) serving:
///   - `GET /ws`       — the AdaTP WebSocket data plane (binary frames)
///   - `GET /healthz`  — liveness
///   - `GET /readyz`   — readiness (DB reachable)
///   - `GET /api/*`    — control-plane endpoints (x-api-key protected)
///
/// The pre-1.0 raw-TCP listener on :8444 has been removed; see docs/legacy.md
/// in the workspace root for the migration note.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // `adatp-server --healthcheck` probes the local /healthz endpoint and
    // exits 0/1 — used as the container HEALTHCHECK (no curl needed).
    if std::env::args().any(|a| a == "--healthcheck") {
        let port = std::env::var("PORT")
            .or_else(|_| std::env::var("SERVER_PORT"))
            .unwrap_or_else(|_| "3000".to_string());
        let url = format!("http://127.0.0.1:{port}/healthz");
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => std::process::exit(0),
            _ => std::process::exit(1),
        }
    }

    let logs = logging::BufLogger::init();

    let cfg = Arc::new(Config::load());
    let metrics = Arc::new(Metrics::new());
    let hub = Arc::new(Hub::new());
    // Fail-closed: the file driver refuses to start without a valid user file
    // (see auth::AuthManager::new). Report clearly and exit non-zero.
    let auth = match AuthManager::new(&cfg) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("FATAL: {e}");
            std::process::exit(1);
        }
    };

    // Ensure the SQLite file exists for sqlite: URLs before connecting.
    if let Some(path) = cfg.database_url.strip_prefix("sqlite:") {
        let path = path.trim_start_matches("//");
        if !path.is_empty() && !std::path::Path::new(path).exists() {
            std::fs::File::create(path)?;
        }
    }
    let db = Arc::new(DbManager::new(&cfg.database_url).await?);

    let plugins = PluginManager::new(&cfg.plugins_dir, hub.clone());
    plugins.load_all().await;

    let webhooks = WebhookManager::start(db.clone(), plugins.clone()).await;
    let load = load::LoadTracker::start(metrics.clone(), hub.clone());
    let admin_token = admin::resolve_admin_token();

    // Long-term identity for the v2 authenticated handshake (generated on first
    // boot). Loaded even on v1-only deployments, where it is simply unused.
    let identity = match identity::ServerIdentity::load_or_create(&cfg.identity_path) {
        Ok(id) => {
            info!(
                "server identity (Ed25519, for v2 handshake pinning): {} [{}]",
                id.fingerprint(),
                cfg.identity_path
            );
            Arc::new(id)
        }
        Err(e) => {
            eprintln!("FATAL: could not load/create server identity at {}: {e}", cfg.identity_path);
            std::process::exit(1);
        }
    };

    let state = Arc::new(AppState {
        metrics,
        db,
        hub: hub.clone(),
        auth,
        cfg: cfg.clone(),
        plugins: plugins.clone(),
        webhooks,
        load,
        identity,
        logs,
        admin_token,
        draining: std::sync::atomic::AtomicBool::new(false),
        conns_in_flight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    });
    plugins.emit_server_event("server.started", serde_json::json!({ "addr": cfg.bind_addr() }));

    let app = api::create_router(state);
    let addr = cfg.bind_addr();
    info!(
        "AdaTP server listening on {} (WebSocket endpoint: /ws, auth driver: {:?})",
        addr, cfg.auth_driver
    );

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!(
                "ERROR: {addr} is already in use by another process.\n\
                 Find it with:  lsof -nP -iTCP:{} -sTCP:LISTEN\n\
                 Or run AdaTP on another port:  PORT=3100 adatp-server",
                cfg.port
            );
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(hub, plugins))
    .await?;

    info!("Server stopped.");
    Ok(())
}

async fn shutdown_signal(hub: Arc<Hub>, plugins: Arc<PluginManager>) {
    // Trap both SIGINT (ctrl-c) and SIGTERM (docker stop / systemd / K8s)
    // so every orchestrator gets a graceful drain.
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("Shutdown signal received; closing {} connection(s)...", hub.connection_count());
    plugins.emit_server_event("server.stopping", serde_json::json!({}));
    plugins.shutdown_all().await;
    hub.shutdown_all();
    // Give connections a moment to flush their Disconnect frames.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}
