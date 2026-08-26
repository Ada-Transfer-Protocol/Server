use axum::{
    http::header,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

/// Silo Panel — the operator UI, embedded into the binary so a single
/// executable ships the whole control room. Served at /silo; every data
/// call it makes goes through the token-protected /admin/v1 API.
pub fn silo_router() -> Router<std::sync::Arc<crate::api::AppState>> {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/i18n.js", get(i18n_js))
        .route("/style.css", get(style_css))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../silo/index.html"))
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../silo/app.js"),
    )
}

async fn i18n_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../silo/i18n.js"),
    )
}

async fn style_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../silo/style.css"),
    )
}
