//! Per-channel authorization — the socket side of Laravel's `/broadcasting/auth`.
//!
//! For a private or presence room, the browser first asks the application server
//! (which knows the user's session) whether it may join. The app server returns a
//! **grant**: a short-lived token, signed with a secret shared with this hub,
//! that names the room, the connection's session id, an expiry, and — for
//! presence — the member's identity. The client presents the grant on `JoinRoom`;
//! the hub verifies it before admitting the join.
//!
//! Crucially, `user_info` comes from the *signed grant*, never from client JSON —
//! otherwise a client could claim to be anyone (an impersonation hole).
//!
//! Grant wire form: `<payload-json>.<hmac-hex>`, where
//! `hmac = HMAC_SHA256(secret, payload-json)`. The exact payload bytes are
//! signed and transmitted, so there is no JSON-canonicalisation gap; the hmac is
//! hex (no `.`), so splitting on the last `.` separates it cleanly.

use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Grant {
    /// The room this grant authorizes.
    pub room: String,
    /// Hex of the connection's 16-byte session id (uuid simple form).
    pub session_id: String,
    /// Expiry, unix milliseconds.
    pub exp: i64,
    /// Stable member id for presence (never trusted from the client directly).
    /// Consumed by presence (member roster / member_added / member_removed).
    #[serde(default)]
    #[allow(dead_code)]
    pub user_id: Option<String>,
    /// Arbitrary member metadata for presence, straight from the signed grant.
    /// Consumed by presence.
    #[serde(default)]
    #[allow(dead_code)]
    pub user_info: Value,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GrantError {
    Malformed,
    BadSignature,
    Expired,
    WrongRoom,
    WrongSession,
}

impl GrantError {
    /// A stable, machine-readable reason (surfaced as the existing
    /// `room_forbidden` close/failure — no new failure shape).
    pub fn reason(&self) -> &'static str {
        match self {
            GrantError::Malformed => "grant_malformed",
            GrantError::BadSignature => "grant_bad_signature",
            GrantError::Expired => "grant_expired",
            GrantError::WrongRoom => "grant_wrong_room",
            GrantError::WrongSession => "grant_wrong_session",
        }
    }
}

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

/// Verify a channel-auth grant against the pinned room and this connection's
/// session id. Returns the parsed grant (with the trusted `user_info`) on
/// success. Checks, in order: signature, well-formedness, expiry, room match,
/// session match.
pub fn verify_grant(
    secret: &str,
    room: &str,
    session_id_hex: &str,
    token: &str,
    now_ms: i64,
) -> Result<Grant, GrantError> {
    let dot = token.rfind('.').ok_or(GrantError::Malformed)?;
    let (payload, sig) = (&token[..dot], &token[dot + 1..]);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let expected: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !ct_eq(sig.as_bytes(), expected.as_bytes()) {
        return Err(GrantError::BadSignature);
    }

    let g: Grant = serde_json::from_str(payload).map_err(|_| GrantError::Malformed)?;
    if g.exp < now_ms {
        return Err(GrantError::Expired);
    }
    if g.room != room {
        return Err(GrantError::WrongRoom);
    }
    if !g.session_id.eq_ignore_ascii_case(session_id_hex) {
        return Err(GrantError::WrongSession);
    }
    Ok(g)
}

/// Sign a grant payload (test/helper mirror of what the app server does).
#[cfg(test)]
pub fn sign_grant(secret: &str, payload_json: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload_json.as_bytes());
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{payload_json}.{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "shared-with-laravel";
    const SID: &str = "0123456789abcdef0123456789abcdef";

    fn grant(room: &str, sid: &str, exp: i64) -> String {
        let payload = format!(
            r#"{{"room":"{room}","session_id":"{sid}","exp":{exp},"user_id":"u7","user_info":{{"name":"Ada"}}}}"#
        );
        sign_grant(SECRET, &payload)
    }

    #[test]
    fn valid_grant_accepts_and_carries_user_info() {
        let t = grant("presence-chat", SID, 9_000_000_000_000);
        let g = verify_grant(SECRET, "presence-chat", SID, &t, 1_700_000_000_000).unwrap();
        assert_eq!(g.user_id.as_deref(), Some("u7"));
        assert_eq!(g.user_info["name"], "Ada");
    }

    #[test]
    fn tampered_payload_fails_signature() {
        let t = grant("presence-chat", SID, 9_000_000_000_000);
        // Flip the room in the payload but keep the old signature.
        let tampered = t.replacen("presence-chat", "presence-admin", 1);
        assert_eq!(
            verify_grant(SECRET, "presence-admin", SID, &tampered, 1_700_000_000_000),
            Err(GrantError::BadSignature)
        );
    }

    #[test]
    fn wrong_room_session_expiry_and_secret_rejected() {
        let t = grant("private-x", SID, 9_000_000_000_000);
        // room the client asked for != room in the grant
        assert_eq!(
            verify_grant(SECRET, "private-y", SID, &t, 1_700_000_000_000),
            Err(GrantError::WrongRoom)
        );
        // different session id
        assert_eq!(
            verify_grant(
                SECRET,
                "private-x",
                "ffffffffffffffffffffffffffffffff",
                &t,
                1_700_000_000_000
            ),
            Err(GrantError::WrongSession)
        );
        // expired
        let old = grant("private-x", SID, 1_000);
        assert_eq!(
            verify_grant(SECRET, "private-x", SID, &old, 1_700_000_000_000),
            Err(GrantError::Expired)
        );
        // wrong secret
        assert_eq!(
            verify_grant("other-secret", "private-x", SID, &t, 1_700_000_000_000),
            Err(GrantError::BadSignature)
        );
    }

    #[test]
    fn malformed_is_rejected() {
        assert_eq!(
            verify_grant(SECRET, "r", SID, "no-dot-here", 1),
            Err(GrantError::Malformed)
        );
    }
}
