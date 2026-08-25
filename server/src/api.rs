use axum::extract::Request;
use axum::{
    extract::{ConnectInfo, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::auth::AuthManager;
use crate::config::Config;
use crate::connection;
use crate::db::DbManager;
use crate::hub::Hub;
use crate::load::LoadTracker;
use crate::logging::BufLogger;
use crate::metrics::Metrics;
use crate::plugins::PluginManager;
use crate::webhooks::WebhookManager;

pub struct AppState {
    pub metrics: Arc<Metrics>,
    pub db: Arc<DbManager>,
    pub hub: Arc<Hub>,
    pub auth: Arc<AuthManager>,
    pub cfg: Arc<Config>,
    pub plugins: Arc<PluginManager>,
    pub webhooks: Arc<WebhookManager>,
    pub load: Arc<LoadTracker>,
    /// Long-term Ed25519 identity for the v2 authenticated handshake; its
    /// public key is what clients pin. Present but unused on v1-only traffic.
    pub identity: Arc<crate::identity::ServerIdentity>,
    pub logs: &'static BufLogger,
    pub admin_token: String,
    /// When true, /readyz reports 503 and new WebSocket connections are
    /// rejected (load-balancer drain).
    pub draining: std::sync::atomic::AtomicBool,
    /// Live count of in-flight WebSocket connections, used to enforce the
    /// `MAX_CONNECTIONS` cap. Reserved at upgrade, released on close.
    pub conns_in_flight: Arc<std::sync::atomic::AtomicUsize>,
}

async fn api_key_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let api_key = headers.get("x-api-key").and_then(|v| v.to_str().ok());
    match api_key {
        Some(key) => match state.db.validate_key(key).await {
            Ok(true) => next.run(request).await,
            _ => (StatusCode::UNAUTHORIZED, "Invalid or inactive API key").into_response(),
        },
        None => (StatusCode::UNAUTHORIZED, "Missing x-api-key header").into_response(),
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let api_routes = Router::new()
        .route("/status", get(status_handler))
        .route("/metrics", get(metrics_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api_key_middleware,
        ));

    Router::new()
        .route("/", get(root_handler))
        .route("/ws", get(ws_handler))
        .route("/healthz", get(healthz_handler))
        .route("/readyz", get(readyz_handler))
        .nest("/api", api_routes)
        .nest("/admin/v1", crate::admin::admin_router(state.clone()))
        .nest("/silo", crate::silo::silo_router())
        // axum's nest maps the inner "/" to exactly "/silo"; cover the
        // trailing-slash form people naturally type.
        .route(
            "/silo/",
            get(|| async { axum::response::Redirect::permanent("/silo") }),
        )
        .with_state(state)
}

async fn root_handler() -> &'static str {
    "AdaTP server is running.\nWebSocket endpoint: /ws\nHealth: /healthz  Readiness: /readyz"
}

async fn healthz_handler() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn readyz_handler(State(state): State<Arc<AppState>>) -> Response {
    if state.draining.load(std::sync::atomic::Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "draining" })),
        )
            .into_response();
    }
    match state.db.ping().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ready" }))).into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn status_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "adatp-server",
        "auth_driver": state.auth.driver_name(),
        "connections": state.hub.connection_count(),
    }))
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snapshot = state.metrics.snapshot();
    let rooms = state.hub.list_rooms();
    Json(json!({
        "uptime_seconds": snapshot.uptime_seconds,
        "active_connections": snapshot.active_connections,
        "total_bytes_received": snapshot.total_bytes_received,
        "total_bytes_sent": snapshot.total_bytes_sent,
        "avg_rx_speed_bps": snapshot.avg_rx_speed_bps,
        "rooms": rooms,
        "dropped_messages": state
            .hub
            .dropped_msgs
            .load(std::sync::atomic::Ordering::Relaxed),
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if state.draining.load(std::sync::atomic::Ordering::Relaxed) {
        return (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response();
    }
    // Connection cap (Finding 3): reserve a slot or reject the new socket.
    let guard = match connection::try_acquire(&state.conns_in_flight, state.cfg.max_connections) {
        Some(g) => g,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "max connections reached").into_response()
        }
    };
    let max = state.cfg.max_frame_bytes + 4096;
    ws.max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| connection::run_ws(socket, state, addr.to_string(), guard))
        .into_response()
}
