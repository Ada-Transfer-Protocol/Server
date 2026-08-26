//! HTTP publish endpoint — `POST /publish`.
//!
//! Lets an application server (e.g. a Laravel broadcast driver) fan a message
//! out to room members over HTTP, the way Pusher/Reverb's publish API does. It
//! is the prerequisite for using AdaTP as a broadcast transport: the app server
//! never holds a socket, it just POSTs signed events.
//!
//! Auth reuses the webhook HMAC-SHA256 scheme (`sha256=<hex>` header), signed
//! over `<timestamp>.<raw-body>` so a replayed body with a stale timestamp is
//! rejected inside the window. The secret is `ADATP_PUBLISH_SECRET`; unset means
//! the endpoint is disabled (503). The routed payload is a JSON envelope
//! `{"event": ..., "data": ...}` delivered as a `TextMessage`; `exclude_session`
//! implements Laravel's `->toOthers()`.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use uuid::Uuid;

use adatp_core::MessageType;

use crate::api::AppState;
use crate::hub::RouteMsg;

const MAX_BODY_BYTES: usize = 1_048_576; // 1 MiB
const MAX_ROOMS_PER_REQUEST: usize = 100;
const REPLAY_WINDOW_MS: i64 = 300_000; // 5 minutes

#[derive(Deserialize)]
struct PublishMsg {
    #[serde(default)]
    rooms: Vec<String>,
    event: String,
    #[serde(default)]
    payload: serde_json::Value,
    /// `->toOthers()`: a client session id to skip when fanning out.
    #[serde(default)]
    exclude_session: Option<String>,
}

/// A publish request is one message or a batch (Laravel broadcasts to several
/// channels at once).
#[derive(Deserialize)]
#[serde(untagged)]
enum PublishBody {
    One(PublishMsg),
    Many(Vec<PublishMsg>),
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "ok": false, "error": code, "message": message })),
    )
        .into_response()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// HMAC-SHA256(secret, "<ts>.<body>") as lowercase hex — the value that goes in
/// `x-adatp-signature: sha256=<...>`. Signing the timestamp with the body is
/// what makes a captured request un-replayable outside the window.
fn compute_sig(secret: &str, ts: i64, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC any key length");
    mac.update(format!("{ts}.").as_bytes());
    mac.update(body);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Constant-time byte comparison for the signature check.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn publish_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let secret = match &state.cfg.publish_secret {
        Some(s) => s,
        None => {
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "publish_disabled",
                "the publish endpoint is disabled (set ADATP_PUBLISH_SECRET)",
            )
        }
    };
    if body.len() > MAX_BODY_BYTES {
        return err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "body_too_large",
            "body exceeds 1 MiB",
        );
    }

    // Timestamp within the replay window.
    let ts = headers
        .get("x-adatp-timestamp")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok());
    let ts = match ts {
        Some(t) => t,
        None => {
            return err(
                StatusCode::BAD_REQUEST,
                "missing_timestamp",
                "x-adatp-timestamp header required",
            )
        }
    };
    if (now_ms() - ts).abs() > REPLAY_WINDOW_MS {
        return err(
            StatusCode::UNAUTHORIZED,
            "stale_timestamp",
            "timestamp outside the replay window",
        );
    }

    // Signature = HMAC-SHA256(secret, "<ts>.<body>"), header `sha256=<hex>`.
    let provided = headers
        .get("x-adatp-signature")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.strip_prefix("sha256=").unwrap_or(s).to_string())
        .unwrap_or_default();
    let expected = compute_sig(secret, ts, &body);
    if !ct_eq(provided.as_bytes(), expected.as_bytes()) {
        return err(
            StatusCode::UNAUTHORIZED,
            "bad_signature",
            "HMAC signature mismatch",
        );
    }

    // Parse (single or batch).
    let parsed: PublishBody = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return err(StatusCode::BAD_REQUEST, "invalid_body", &e.to_string()),
    };
    let msgs = match parsed {
        PublishBody::One(m) => vec![m],
        PublishBody::Many(v) => v,
    };
    let total_rooms: usize = msgs.iter().map(|m| m.rooms.len()).sum();
    if total_rooms > MAX_ROOMS_PER_REQUEST {
        return err(
            StatusCode::BAD_REQUEST,
            "too_many_rooms",
            "at most 100 room targets per request",
        );
    }

    // Fan out. Each connection re-encrypts under its own session keys, exactly
    // as an in-band room broadcast does.
    let mut results = Vec::new();
    for m in &msgs {
        let exclude = m
            .exclude_session
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok());
        let envelope = json!({ "event": m.event, "data": m.payload }).to_string();
        let route = RouteMsg {
            sender: Uuid::nil(), // app-server origin
            msg_type: MessageType::TextMessage,
            payload: Bytes::from(envelope),
        };
        for room in &m.rooms {
            let delivered = state.hub.broadcast_publish(room, route.clone(), exclude);
            results.push(json!({ "room": room, "event": m.event, "delivered": delivered }));
        }
    }

    (
        StatusCode::OK,
        Json(json!({ "ok": true, "results": results })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_deterministic_and_binds_timestamp_and_body() {
        let s = compute_sig("secret", 1_700_000_000_000, b"{\"event\":\"x\"}");
        // Stable, 64 hex chars.
        assert_eq!(s.len(), 64);
        assert_eq!(
            s,
            compute_sig("secret", 1_700_000_000_000, b"{\"event\":\"x\"}")
        );
        // A different timestamp, body, or secret changes it (no replay).
        assert_ne!(
            s,
            compute_sig("secret", 1_700_000_000_001, b"{\"event\":\"x\"}")
        );
        assert_ne!(
            s,
            compute_sig("secret", 1_700_000_000_000, b"{\"event\":\"y\"}")
        );
        assert_ne!(
            s,
            compute_sig("other", 1_700_000_000_000, b"{\"event\":\"x\"}")
        );
    }

    #[test]
    fn ct_eq_matches_and_rejects() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
