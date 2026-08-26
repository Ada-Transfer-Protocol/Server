use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::sse::{Event, Sse},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use log::{info, warn};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::api::AppState;

/// Admin control plane under /admin/v1.
///
/// Authentication: every request must carry the admin token in
/// `x-admin-token` or `Authorization: Bearer <token>`. The token comes from
/// ADMIN_TOKEN, or is generated at boot and printed once to the log.
pub fn admin_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/overview", get(overview))
        .route("/connections", get(connections))
        .route("/connections/:id", delete(kick_connection))
        .route("/rooms", get(rooms))
        .route("/logs", get(logs_recent))
        .route("/logs/stream", get(logs_stream))
        .route("/config", get(config_view))
        .route("/load", get(load_view))
        .route("/lb-hints", get(lb_hints))
        .route("/drain", post(drain))
        .route("/webhooks", get(webhooks_list).post(webhooks_create))
        .route("/webhooks/audit", get(webhooks_audit))
        .route(
            "/webhooks/:id",
            axum::routing::patch(webhooks_patch).delete(webhooks_delete),
        )
        .route("/webhooks/:id/test", post(webhooks_test))
        .route("/plugins", get(plugins_list))
        .route("/plugins/:name/enable", post(plugin_enable))
        .route("/plugins/:name/disable", post(plugin_disable))
        .route("/plugins/:name/reload", post(plugin_reload))
        .route("/users/reload", post(users_reload))
        .route_layer(middleware::from_fn_with_state(state, admin_auth))
}

/// Resolves the admin token at boot.
pub fn resolve_admin_token() -> String {
    match std::env::var("ADMIN_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => {
            let t = Uuid::new_v4().simple().to_string();
            warn!("ADMIN_TOKEN not set — generated one for this run: {t}");
            warn!("Set ADMIN_TOKEN in the environment to keep it stable.");
            t
        }
    }
}

async fn admin_auth(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    // EventSource cannot set headers, so the log stream may pass the token
    // as a query parameter instead.
    let query_token = request.uri().query().and_then(|q| {
        q.split('&')
            .find_map(|kv| kv.strip_prefix("token="))
            .map(|v| v.to_string())
    });

    let presented = headers
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        })
        .or(query_token);

    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), state.admin_token.as_bytes()) => {
            next.run(request).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_admin_token" })),
        )
            .into_response(),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------------------------------------------------------------------
// Overview / observability
// ---------------------------------------------------------------------------

async fn overview(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snap = state.metrics.snapshot();
    let plugins = state.plugins.list();
    let running = plugins
        .iter()
        .filter(|p| matches!(p.state, crate::plugins::PluginState::Running))
        .count();
    let webhooks = state.webhooks.views().await;

    Json(json!({
        "service": "adatp-server",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": snap.uptime_seconds,
        "draining": state.draining.load(Ordering::Relaxed),
        "auth_driver": state.auth.driver_name(),
        "connections": {
            "active": state.hub.connection_count(),
            "dropped_messages": state.hub.dropped_msgs.load(Ordering::Relaxed),
        },
        "rooms": state.hub.list_rooms().len(),
        "traffic": {
            "total_bytes_received": snap.total_bytes_received,
            "total_bytes_sent": snap.total_bytes_sent,
        },
        "plugins": { "total": plugins.len(), "running": running },
        "webhooks": {
            "total": webhooks.len(),
            "active": webhooks.iter().filter(|w| w.is_active).count(),
        },
    }))
}

async fn connections(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "connections": state.hub.list_connections() }))
}

async fn kick_connection(State(state): State<Arc<AppState>>, Path(id): Path<u64>) -> Response {
    if state.hub.kick(id) {
        info!("Admin kicked connection {id}");
        Json(json!({ "ok": true })).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown_connection" })),
        )
            .into_response()
    }
}

async fn rooms(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "rooms": state.hub.list_rooms() }))
}

async fn logs_recent(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "lines": state.logs.recent(200) }))
}

async fn logs_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.logs.subscribe();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(line) => {
                    let event = Event::default()
                        .json_data(&line)
                        .unwrap_or_else(|_| Event::default().data("{}"));
                    return Some((Ok(event), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn config_view(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    // Non-secret configuration only: no tokens, keys, or passwords.
    let cfg = &state.cfg;
    Json(json!({
        "host": cfg.host,
        "port": cfg.port,
        "auth_driver": state.auth.driver_name(),
        "auth_file_path": cfg.auth_file_path,
        "auth_api_url_set": cfg.auth_api_url.is_some(),
        "database_url": cfg.database_url,
        "max_frame_bytes": cfg.max_frame_bytes,
        "idle_timeout_secs": cfg.idle_timeout_secs,
        "plugins_dir": cfg.plugins_dir,
    }))
}

async fn load_view(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "current": state.load.current(),
        "series": state.load.series(),
    }))
}

async fn lb_hints(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let max_connections: usize = std::env::var("MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let active = state.hub.connection_count();
    let draining = state.draining.load(Ordering::Relaxed);
    Json(json!({
        "healthy": !draining,
        "draining": draining,
        "connections": active,
        "max_connections": max_connections,
        "capacity_used_pct": (active as f64 / max_connections as f64 * 100.0).round(),
    }))
}

#[derive(Deserialize)]
struct DrainBody {
    enabled: bool,
    /// Also ask existing connections to close gracefully.
    #[serde(default)]
    disconnect_clients: bool,
}

async fn drain(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DrainBody>,
) -> Json<serde_json::Value> {
    state.draining.store(body.enabled, Ordering::Relaxed);
    info!(
        "Admin set drain={} (disconnect_clients={})",
        body.enabled, body.disconnect_clients
    );
    if body.enabled && body.disconnect_clients {
        state.hub.shutdown_all();
    }
    Json(json!({ "ok": true, "draining": body.enabled }))
}

// ---------------------------------------------------------------------------
// Webhooks CRUD
// ---------------------------------------------------------------------------

async fn webhooks_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "webhooks": state.webhooks.views().await }))
}

#[derive(Deserialize)]
struct WebhookCreateBody {
    url: String,
    #[serde(default)]
    events: Vec<String>,
    /// Generated when omitted; returned once in the response.
    secret: Option<String>,
    description: Option<String>,
}

async fn webhooks_create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WebhookCreateBody>,
) -> Response {
    if let Err(e) = state.webhooks.validate_url(&body.url).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_url", "detail": e })),
        )
            .into_response();
    }
    let events = if body.events.is_empty() {
        vec!["*".to_string()]
    } else {
        body.events
    };
    let secret = body
        .secret
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());

    match state
        .db
        .webhook_create(&body.url, &secret, &events, body.description.as_deref())
        .await
    {
        Ok(row) => {
            state.webhooks.refresh().await;
            info!("Admin created webhook {} → {}", row.id, row.url);
            // The secret is shown exactly once, at creation time.
            Json(json!({ "ok": true, "id": row.id, "secret": secret })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "db_error", "detail": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct WebhookPatchBody {
    active: bool,
}

async fn webhooks_patch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<WebhookPatchBody>,
) -> Response {
    match state.db.webhook_set_active(&id, body.active).await {
        Ok(true) => {
            state.webhooks.refresh().await;
            Json(json!({ "ok": true })).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown_webhook" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "db_error", "detail": e.to_string() })),
        )
            .into_response(),
    }
}

async fn webhooks_delete(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.db.webhook_delete(&id).await {
        Ok(true) => {
            state.webhooks.refresh().await;
            info!("Admin deleted webhook {id}");
            Json(json!({ "ok": true })).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown_webhook" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "db_error", "detail": e.to_string() })),
        )
            .into_response(),
    }
}

async fn webhooks_test(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match state.webhooks.send_test(&id).await {
        Ok(()) => Json(json!({ "ok": true, "queued": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

async fn webhooks_audit(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "audit": state.webhooks.audit_log().await }))
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

async fn plugins_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "plugins": state.plugins.list() }))
}

async fn plugin_enable(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    match state.plugins.enable(&name).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

async fn plugin_disable(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    match state.plugins.disable(&name).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

async fn plugin_reload(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    match state.plugins.reload(&name).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

async fn users_reload(State(state): State<Arc<AppState>>) -> Response {
    match state.auth.reload().await {
        Ok(n) => Json(json!({ "ok": true, "users": n })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}
