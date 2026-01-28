use axum::{
    routing::get,
    Router,
    Json,
    extract::State,
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response},
    middleware::{self, Next},
};
use axum::extract::Request;
use std::sync::Arc;
use serde_json::json;

use crate::metrics::Metrics;
use crate::db::DbManager;

pub struct AppState {
    pub metrics: Arc<Metrics>,
    pub db: Arc<DbManager>,
}

// Middleware to check API Key
async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let api_key = headers
        .get("x-api-key")
        .and_then(|val| val.to_str().ok());

    match api_key {
        Some(key) => {
            // Check DB
            match state.db.validate_key(key).await {
                Ok(true) => next.run(request).await,
                _ => (StatusCode::UNAUTHORIZED, "Invalid or Inactive API Key").into_response(),
            }
        }
        None => (StatusCode::UNAUTHORIZED, "Missing x-api-key header").into_response(),
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let api_routes = Router::new()
        .route("/status", get(status_handler))
        .route("/metrics", get(metrics_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));
        
    Router::new()
        .route("/", get(root_handler))
        .nest("/api", api_routes)
        .with_state(state)
}

async fn root_handler() -> &'static str {
    "AdaTP Server is running! 🚀"
}

async fn status_handler() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "adatp-server" }))
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snapshot = state.metrics.snapshot();
    Json(json!(snapshot))
}
