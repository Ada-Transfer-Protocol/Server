use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use log::{debug, info, warn};
use tokio::sync::mpsc;
use uuid::Uuid;

use adatp_core::crypto::key_derivation::SessionKeys;
use adatp_core::crypto::x25519::{diffie_hellman, KeyPair};
use adatp_core::session::handshake_v2;
use adatp_core::session::secure_session::{Role, SecureSession};
use adatp_core::{MessageType, Packet, PacketFlags};

use crate::api::AppState;
use crate::auth::{AuthError, AuthRequestBody, AuthUser};
use crate::hub::{ConnId, OutEvent, RouteMsg};

/// HKDF salt for session key derivation. All SDKs use 32 zero bytes.
const KDF_SALT: [u8; 32] = [0u8; 32];

const MAX_AUTH_ATTEMPTS: u8 = 3;
const MAX_PREAUTH_VIOLATIONS: u8 = 10;
const OUT_QUEUE_CAPACITY: usize = 256;

/// RAII slot for the concurrent-connection cap (`MAX_CONNECTIONS`). The count
/// is incremented in [`try_acquire`] and decremented here on drop — including
/// when the upgrade future is dropped before it ever reaches [`run_ws`], so a
/// rejected or abandoned upgrade never leaks a slot.
pub struct ConnGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Reserve a connection slot, or return `None` when the live count already sits
/// at `max`. Increments first and rolls back on overflow so there is no
/// check-then-act race between concurrent upgrades.
pub fn try_acquire(counter: &Arc<AtomicUsize>, max: usize) -> Option<ConnGuard> {
    let prev = counter.fetch_add(1, Ordering::AcqRel);
    if prev >= max {
        counter.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    Some(ConnGuard {
        counter: counter.clone(),
    })
}

/// Per-connection token-bucket limiter for inbound messages (`MSG_RATE_LIMIT`).
/// `rate_per_sec` is both the steady-state rate and the burst capacity; a rate
/// of `0` disables limiting.
struct RateLimiter {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last: Instant,
    enabled: bool,
}

impl RateLimiter {
    fn new(rate_per_sec: u32) -> Self {
        let cap = rate_per_sec.max(1) as f64;
        Self {
            tokens: cap,
            capacity: cap,
            refill_per_sec: rate_per_sec as f64,
            last: Instant::now(),
            enabled: rate_per_sec > 0,
        }
    }

    /// Consume one token for an inbound message. Returns `false` when the
    /// bucket is empty (the connection is over its rate and should be closed).
    fn allow(&mut self, now: Instant) -> bool {
        if !self.enabled {
            return true;
        }
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-connection protocol state.
struct ConnState {
    /// Client identity for routing — captured from the first packet header
    /// and pinned for the lifetime of the connection (a client cannot switch
    /// identity mid-stream).
    session_id: Option<Uuid>,
    /// Present once a HandshakeInit with a valid X25519 key was answered.
    secure: Option<SecureSession>,
    /// For a **v2** (authenticated) handshake: the transcript hash the server
    /// signed, kept so the client's encrypted `HandshakeComplete` can be checked
    /// to confirm the same key/transcript (key confirmation). `None` for a v1
    /// handshake, which has no such confirmation step.
    pending_v2_th: Option<[u8; 32]>,
    /// True once HandshakeComplete decrypted successfully; from then on all
    /// server->client packets are encrypted.
    secure_established: bool,
    authed: Option<AuthUser>,
    hub_id: Option<ConnId>,
    room: String,
    /// Verified channel-auth grant for the current private/presence room, if any.
    /// Its `user_info` is the trusted source of presence identity.
    channel_grant: Option<crate::channel_auth::Grant>,
    /// The `presence-*` room this connection is currently a roster member of, if
    /// any. Tracked so a room change or disconnect can retract the membership and
    /// announce `member_removed`.
    presence_room: Option<String>,
    auth_attempts: u8,
    preauth_violations: u8,
    /// Inbound message rate limiter (per connection).
    rate: RateLimiter,
}

impl ConnState {
    fn new(msg_rate_limit: u32) -> Self {
        Self {
            session_id: None,
            secure: None,
            pending_v2_th: None,
            secure_established: false,
            authed: None,
            hub_id: None,
            room: "global".to_string(),
            channel_grant: None,
            presence_room: None,
            auth_attempts: 0,
            preauth_violations: 0,
            rate: RateLimiter::new(msg_rate_limit),
        }
    }

    fn sid(&self) -> Uuid {
        self.session_id.unwrap_or_else(Uuid::nil)
    }
}

/// What the packet handler wants the main loop to do next.
enum Flow {
    Continue,
    Close(&'static str),
}

/// Drive one WebSocket connection. `_conn_guard` holds the connection-cap slot
/// for the whole lifetime of the connection and releases it on return.
pub async fn run_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    remote: String,
    _conn_guard: ConnGuard,
) {
    state.metrics.inc_connection();
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<OutEvent>(OUT_QUEUE_CAPACITY);

    let mut conn = ConnState::new(state.cfg.msg_rate_limit);
    let mut last_rx = Instant::now();
    let idle_timeout = Duration::from_secs(state.cfg.idle_timeout_secs);
    let mut keepalive = tokio::time::interval(Duration::from_secs(30));
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let close_reason: &'static str = loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let now = Instant::now();
                        last_rx = now;
                        state.metrics.add_rx(data.len() as u64);
                        // Per-connection message rate limit (Finding 3): a
                        // connection over its budget is soft-closed.
                        if !conn.rate.allow(now) {
                            warn!("Connection {remote} exceeded message rate; closing");
                            break "rate_limited";
                        }
                        if data.len() > state.cfg.max_frame_bytes + 128 {
                            break "frame_too_large";
                        }
                        match handle_packet(&state, &mut conn, &out_tx, &remote, data, &mut ws_tx).await {
                            Flow::Continue => {}
                            Flow::Close(reason) => break reason,
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        last_rx = Instant::now();
                        let _ = ws_tx.send(Message::Pong(p)).await;
                    }
                    Some(Ok(Message::Pong(_))) => { last_rx = Instant::now(); }
                    Some(Ok(Message::Text(_))) => {
                        // AdaTP is binary-only over WebSocket; text frames are ignored.
                        last_rx = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => break "peer_closed",
                    Some(Err(e)) => {
                        debug!("WS read error from {remote}: {e}");
                        break "read_error";
                    }
                }
            }

            out = out_rx.recv() => {
                match out {
                    Some(OutEvent::Route(m)) => {
                        if conn.authed.is_some() {
                            let bytes = encode_for_peer(&mut conn, m.msg_type, &m.payload, m.sender);
                            state.metrics.add_tx(bytes.len() as u64);
                            if ws_tx.send(Message::Binary(bytes)).await.is_err() {
                                break "write_error";
                            }
                        }
                    }
                    Some(OutEvent::Shutdown) => {
                        let sid = conn.sid();
                        let bye = encode_for_peer(&mut conn, MessageType::Disconnect, b"server_shutdown", sid);
                        let _ = ws_tx.send(Message::Binary(bye)).await;
                        break "server_shutdown";
                    }
                    None => break "queue_closed",
                }
            }

            _ = keepalive.tick() => {
                if last_rx.elapsed() > idle_timeout {
                    break "idle_timeout";
                }
                // WS protocol-level ping; every SDK/browser answers automatically.
                if ws_tx.send(Message::Ping(Vec::new())).await.is_err() {
                    break "write_error";
                }
            }
        }
    };

    // Controlled close & presence cleanup.
    // Rich presence first: retract this connection's roster membership and, if it
    // was the user's last session, announce member_removed to the room.
    if let Some(proom) = conn.presence_room.take() {
        if let Some(gone) = state.hub.presence_leave(&proom, conn.sid()) {
            let evt = serde_json::json!({
                "event": "presence:member_removed", "member": gone,
            })
            .to_string();
            state.hub.broadcast(
                &proom,
                RouteMsg {
                    sender: conn.sid(),
                    msg_type: MessageType::PresenceUpdate,
                    payload: Bytes::from(evt.into_bytes()),
                },
            );
        }
    }
    if let Some(id) = conn.hub_id {
        if let Some((room, session_id)) = state.hub.unregister(id) {
            state.hub.broadcast(
                &room,
                RouteMsg {
                    sender: session_id,
                    msg_type: MessageType::PresenceUpdate,
                    payload: Bytes::from_static(b"LEAVE"),
                },
            );
            if state.plugins.has_hook("leave") {
                let event = serde_json::json!({ "room": room, "sender": sender_ctx(&conn) });
                state.plugins.notify_hook("leave", &event).await;
            }
        }
    }
    if let Some(user) = conn.authed.as_ref() {
        state.plugins.emit_server_event(
            "connection.closed",
            serde_json::json!({
                "username": user.username, "remote": remote, "reason": close_reason,
            }),
        );
    }
    let _ = ws_tx.send(Message::Close(None)).await;
    state.metrics.dec_connection();
    info!(
        "Connection {} closed ({}): user={}",
        remote,
        close_reason,
        conn.authed
            .as_ref()
            .map(|u| u.username.as_str())
            .unwrap_or("-")
    );
}

/// Encode a server->client packet, encrypting when the session is secure.
fn encode_for_peer(
    conn: &mut ConnState,
    msg_type: MessageType,
    plaintext: &[u8],
    stamp_session: Uuid,
) -> Vec<u8> {
    if conn.secure_established {
        if let Some(secure) = conn.secure.as_mut() {
            // Build the header first; encrypt fills seq/length/flags and (for v2)
            // binds the finalized header as AEAD AAD.
            let mut pkt = Packet::new(msg_type, Bytes::new(), stamp_session);
            if let Ok((ciphertext, tag)) = secure.encrypt(plaintext, &mut pkt.header) {
                pkt.payload = Bytes::from(ciphertext);
                pkt.auth_tag = Some(tag);
                return pkt.to_bytes().to_vec();
            }
            warn!("encrypt failed; dropping to plaintext close");
        }
    }
    Packet::new(msg_type, Bytes::copy_from_slice(plaintext), stamp_session)
        .to_bytes()
        .to_vec()
}

async fn send_direct(
    state: &Arc<AppState>,
    conn: &mut ConnState,
    ws_tx: &mut (impl SinkExt<Message> + Unpin),
    msg_type: MessageType,
    payload: &[u8],
) -> bool {
    let sid = conn.sid();
    let bytes = encode_for_peer(conn, msg_type, payload, sid);
    state.metrics.add_tx(bytes.len() as u64);
    ws_tx.send(Message::Binary(bytes)).await.is_ok()
}

fn is_routable(t: MessageType) -> bool {
    matches!(
        t,
        MessageType::TextMessage
            | MessageType::TextAck
            | MessageType::TextRead
            | MessageType::FileInit
            | MessageType::FileChunk
            | MessageType::FileAck
            | MessageType::FileComplete
            | MessageType::FileCancel
            | MessageType::VoiceInit
            | MessageType::VoiceOffer
            | MessageType::VoiceAnswer
            | MessageType::VoiceIce
            | MessageType::VoiceData
            | MessageType::VoiceEnd
            | MessageType::GameState
            | MessageType::VideoInit
            | MessageType::VideoOffer
            | MessageType::VideoAnswer
            | MessageType::VideoData
            | MessageType::VideoEnd
            | MessageType::PresenceUpdate
            | MessageType::TypingIndicator
    )
}

fn valid_room_name(name: &str) -> bool {
    let len = name.len();
    (1..=128).contains(&len) && !name.chars().any(|c| c.is_control())
}

/// Parse a JoinRoom payload. Back-compatible: a bare UTF-8 string is the room
/// name; a JSON object `{"room":"...","grant":"...","recover":true}` additionally
/// carries a channel-auth grant and/or a request to replay missed messages on
/// reconnect. Returns `(room, grant?, recover)`; an empty room signals a
/// malformed payload (rejected upstream).
fn parse_join_payload(bytes: &[u8]) -> (String, Option<String>, bool) {
    let first = bytes.iter().find(|b| !b.is_ascii_whitespace());
    if first == Some(&b'{') {
        #[derive(serde::Deserialize)]
        struct JoinBody {
            room: String,
            #[serde(default)]
            grant: Option<String>,
            #[serde(default)]
            recover: bool,
        }
        return match serde_json::from_slice::<JoinBody>(bytes) {
            Ok(b) => (b.room, b.grant, b.recover),
            Err(_) => (String::new(), None, false),
        };
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), None, false),
        Err(_) => (String::new(), None, false),
    }
}

async fn handle_packet(
    state: &Arc<AppState>,
    conn: &mut ConnState,
    out_tx: &mpsc::Sender<OutEvent>,
    remote: &str,
    data: Vec<u8>,
    ws_tx: &mut (impl SinkExt<Message> + Unpin),
) -> Flow {
    let packet = match Packet::from_bytes(Bytes::from(data)) {
        Ok(p) => p,
        Err(e) => {
            debug!("Malformed packet from {remote}: {e}");
            return Flow::Close("malformed_packet");
        }
    };

    if packet.payload.len() > state.cfg.max_frame_bytes {
        return Flow::Close("frame_too_large");
    }

    // Pin the client identity to the first packet's session id.
    if conn.session_id.is_none() {
        let sid = packet.header.session_id;
        conn.session_id = Some(if sid.is_nil() { Uuid::new_v4() } else { sid });
    }

    match packet.header.msg_type {
        MessageType::HandshakeInit => {
            if conn.authed.is_some() || conn.secure.is_some() {
                return Flow::Close("handshake_replay");
            }
            // Downgrade floor: reject a handshake below the configured minimum
            // protocol version. With ADATP_MIN_PROTOCOL_VERSION=2 a client cannot
            // fall back to the unauthenticated v1 flow (it must run authenticated
            // v2), which is the server-side half of the downgrade defense.
            if packet.header.version < state.cfg.min_protocol_version {
                debug!(
                    "rejecting handshake from {remote}: version {} < min {}",
                    packet.header.version, state.cfg.min_protocol_version
                );
                return Flow::Close("protocol_version_too_low");
            }
            if packet.payload.len() >= 32 {
                let mut epk_c = [0u8; 32];
                epk_c.copy_from_slice(&packet.payload[..32]);

                if packet.header.version >= handshake_v2::PROTOCOL_V2 {
                    // ---- Protocol v2: authenticated handshake ----
                    // The server signs the transcript (which binds both
                    // ephemerals + its identity) with its long-term Ed25519 key;
                    // a client that pinned spk_S verifies before deriving keys.
                    // This is the ProVerif-verified flow that closes v1's MITM
                    // (docs/spec/12-authenticated-handshake.md + docs/spec/formal/).
                    match handshake_v2::server_respond(state.identity.keypair(), &epk_c, &KDF_SALT)
                    {
                        Ok(sh) => {
                            // v2 session: header bound as AEAD AAD.
                            conn.secure = Some(SecureSession::new_v2(Role::Server, sh.keys));
                            // Remember th to verify the client's encrypted
                            // HandshakeComplete confirmation.
                            conn.pending_v2_th = Some(sh.transcript_hash);
                            // Signed ServerHello (epk_s || spk_s || sig), stamped
                            // version=2 so the peer sees the negotiated protocol.
                            let sid = conn.sid();
                            let mut pkt = Packet::new(
                                MessageType::HandshakeResponse,
                                Bytes::from(sh.response),
                                sid,
                            );
                            pkt.header.version = handshake_v2::PROTOCOL_V2;
                            pkt.header.length = pkt.payload.len() as u32;
                            let bytes = pkt.to_bytes().to_vec();
                            state.metrics.add_tx(bytes.len() as u64);
                            if ws_tx.send(Message::Binary(bytes)).await.is_err() {
                                return Flow::Close("write_error");
                            }
                        }
                        Err(_) => return Flow::Close("bad_handshake_key"),
                    }
                } else {
                    // ---- Protocol v1: unauthenticated X25519 (unchanged) ----
                    let kp = KeyPair::generate();
                    let server_pub = *kp.public.as_bytes();
                    match diffie_hellman(kp.secret, &epk_c) {
                        Ok(shared) => {
                            let keys = SessionKeys::derive(&shared, &KDF_SALT);
                            conn.secure = Some(SecureSession::new(Role::Server, keys));
                            if !send_direct(
                                state,
                                conn,
                                ws_tx,
                                MessageType::HandshakeResponse,
                                &server_pub,
                            )
                            .await
                            {
                                return Flow::Close("write_error");
                            }
                        }
                        Err(_) => return Flow::Close("bad_handshake_key"),
                    }
                }
            } else {
                // Plaintext-mode hello (e.g. "AdaTP v1.0"): acknowledge without
                // key material — the session simply stays unencrypted.
                if !send_direct(state, conn, ws_tx, MessageType::HandshakeResponse, &[]).await {
                    return Flow::Close("write_error");
                }
            }
            Flow::Continue
        }

        MessageType::HandshakeComplete => {
            if packet.header.flags.contains(PacketFlags::ENCRYPTED) {
                // Decrypt the client's confirmation under the freshly derived key.
                let plaintext = match conn.secure.as_mut() {
                    Some(secure) => match secure.decrypt(&packet) {
                        Ok(p) => p,
                        Err(_) => return Flow::Close("handshake_verify_failed"),
                    },
                    None => return Flow::Close("handshake_verify_failed"),
                };
                // For a v2 handshake the confirmation MUST be Finished =
                // FINISHED_LABEL || th, proving the client derived the same key
                // for the same signed transcript (key confirmation). v1 has no
                // such step, so this check is skipped there.
                if let Some(th) = conn.pending_v2_th {
                    if !handshake_v2::verify_finished(&th, &plaintext) {
                        return Flow::Close("handshake_verify_failed");
                    }
                }
                conn.secure_established = true;
                debug!("Secure session established for {remote}");
            }
            Flow::Continue
        }

        MessageType::AuthRequest => {
            let plaintext = match decrypt_in(conn, &packet) {
                Ok(p) => p,
                Err(_) => return Flow::Close("decrypt_failed"),
            };
            let body: AuthRequestBody = match serde_json::from_slice(&plaintext) {
                Ok(b) => b,
                Err(_) => {
                    conn.auth_attempts += 1;
                    let _ = send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::AuthFailure,
                        br#"{"error":"malformed_auth_request"}"#,
                    )
                    .await;
                    return if conn.auth_attempts >= MAX_AUTH_ATTEMPTS {
                        Flow::Close("auth_failed")
                    } else {
                        Flow::Continue
                    };
                }
            };

            match state
                .auth
                .verify(&body.username, &body.password, body.auth_string.as_deref())
                .await
            {
                Ok(user) => {
                    // Policy plugins may veto an otherwise-valid login.
                    if state.plugins.has_hook("auth") {
                        let event = serde_json::json!({
                            "username": user.username, "role": user.role, "remote": remote,
                        });
                        if !state.plugins.veto_hook("auth", &event).await {
                            conn.auth_attempts += 1;
                            warn!(
                                "Auth vetoed by plugin for {remote} (user '{}')",
                                user.username
                            );
                            state.plugins.emit_server_event("auth.failure", serde_json::json!({
                                "username": user.username, "remote": remote, "reason": "forbidden",
                            }));
                            let _ = send_direct(
                                state,
                                conn,
                                ws_tx,
                                MessageType::AuthFailure,
                                br#"{"error":"forbidden"}"#,
                            )
                            .await;
                            return if conn.auth_attempts >= MAX_AUTH_ATTEMPTS {
                                Flow::Close("auth_failed")
                            } else {
                                Flow::Continue
                            };
                        }
                    }

                    let ok_payload = serde_json::json!({
                        "user_id": user.user_id,
                        "username": user.username,
                        "role": user.role,
                    })
                    .to_string();

                    if conn.authed.is_none() {
                        let id = state.hub.register(
                            conn.sid(),
                            user.username.clone(),
                            user.role.clone(),
                            conn.room.clone(),
                            remote.to_string(),
                            out_tx.clone(),
                        );
                        conn.hub_id = Some(id);
                    }
                    conn.authed = Some(user.clone());
                    info!(
                        "Auth success for {remote}: {} (role {})",
                        user.username, user.role
                    );
                    state.plugins.emit_server_event(
                        "auth.success",
                        serde_json::json!({
                            "username": user.username, "role": user.role, "remote": remote,
                        }),
                    );
                    if !send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::AuthSuccess,
                        ok_payload.as_bytes(),
                    )
                    .await
                    {
                        return Flow::Close("write_error");
                    }
                    Flow::Continue
                }
                Err(AuthError::InvalidCredentials) => {
                    conn.auth_attempts += 1;
                    warn!("Auth failure for {remote} (user '{}')", body.username);
                    state.plugins.emit_server_event("auth.failure", serde_json::json!({
                        "username": body.username, "remote": remote, "reason": "invalid_credentials",
                    }));
                    let _ = send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::AuthFailure,
                        br#"{"error":"invalid_credentials"}"#,
                    )
                    .await;
                    if conn.auth_attempts >= MAX_AUTH_ATTEMPTS {
                        Flow::Close("auth_failed")
                    } else {
                        Flow::Continue
                    }
                }
                Err(AuthError::Unavailable(e)) => {
                    warn!("Auth backend unavailable: {e}");
                    let _ = send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::AuthFailure,
                        br#"{"error":"auth_unavailable"}"#,
                    )
                    .await;
                    // Fail closed: never admit clients while the backend is down.
                    Flow::Close("auth_unavailable")
                }
            }
        }

        MessageType::Ping => {
            let payload = packet.payload.to_vec();
            if !send_direct(state, conn, ws_tx, MessageType::Pong, &payload).await {
                return Flow::Close("write_error");
            }
            Flow::Continue
        }

        MessageType::Disconnect => Flow::Close("client_disconnect"),

        MessageType::JoinRoom => {
            if conn.authed.is_none() {
                return unauthorized(state, conn, ws_tx).await;
            }
            let plaintext = match decrypt_in(conn, &packet) {
                Ok(p) => p,
                Err(_) => return Flow::Close("decrypt_failed"),
            };
            // The JoinRoom payload is either a bare room name, or a JSON object
            // { "room": "...", "grant": "..." } carrying a channel-auth grant for
            // a private/presence room (the socket side of /broadcasting/auth).
            let (room, grant_token, recover) = parse_join_payload(&plaintext);
            if !valid_room_name(&room) {
                let _ = send_direct(
                    state,
                    conn,
                    ws_tx,
                    MessageType::AuthFailure,
                    br#"{"error":"invalid_room_name"}"#,
                )
                .await;
                return Flow::Continue;
            }

            // --- Room authorization (Finding 2) --------------------------
            // Being authenticated is not enough to enter any room. Two real,
            // independent gates run before the join takes effect; the default
            // configuration leaves both permissive (public rooms).
            //
            // (a) Built-in config policy: optional allowlist and a
            //     protected-prefix role requirement (see Config::room_join_allowed).
            let role = conn
                .authed
                .as_ref()
                .map(|u| u.role.clone())
                .unwrap_or_default();
            if let Err(reason) = state.cfg.room_join_allowed(&room, &role) {
                warn!(
                    "Room join denied by policy: user '{}' → '{}' ({reason})",
                    conn.authed.as_ref().unwrap().username,
                    room
                );
                state.plugins.emit_server_event("room.denied", serde_json::json!({
                    "room": room, "username": conn.authed.as_ref().unwrap().username, "reason": reason,
                }));
                let body = format!(r#"{{"error":"{reason}"}}"#);
                let _ = send_direct(
                    state,
                    conn,
                    ws_tx,
                    MessageType::AuthFailure,
                    body.as_bytes(),
                )
                .await;
                return Flow::Continue;
            }

            // (c) Per-channel authorization: a private/presence room requires a
            //     valid channel-auth grant (signed by the app server) that names
            //     this room and this connection's session id and has not expired.
            //     The member's user_info for presence comes only from the verified
            //     grant, never from client JSON. Failure returns the existing
            //     room_forbidden path — no new failure shape.
            if state.cfg.room_requires_grant(&room) {
                let secret = state.cfg.channel_auth_secret.as_deref().unwrap_or("");
                let sid_hex = conn
                    .session_id
                    .map(|u| u.simple().to_string())
                    .unwrap_or_default();
                let now = chrono::Utc::now().timestamp_millis();
                let verdict = match grant_token.as_deref() {
                    Some(tok) => {
                        crate::channel_auth::verify_grant(secret, &room, &sid_hex, tok, now)
                    }
                    None => Err(crate::channel_auth::GrantError::Malformed),
                };
                match verdict {
                    Ok(grant) => conn.channel_grant = Some(grant),
                    Err(e) => {
                        warn!(
                            "Room join denied (grant {}): user '{}' → '{}'",
                            e.reason(),
                            conn.authed.as_ref().unwrap().username,
                            room
                        );
                        state.plugins.emit_server_event(
                            "room.denied",
                            serde_json::json!({
                                "room": room,
                                "username": conn.authed.as_ref().unwrap().username,
                                "reason": e.reason(),
                            }),
                        );
                        let _ = send_direct(
                            state,
                            conn,
                            ws_tx,
                            MessageType::AuthFailure,
                            br#"{"error":"room_forbidden"}"#,
                        )
                        .await;
                        return Flow::Continue;
                    }
                }
            } else {
                // A public room: drop any grant carried over from a prior
                // private/presence room so `channel_grant` tracks the current room.
                conn.channel_grant = None;
            }

            // (b) Policy plugins may veto the join. `join` is a veto hook: a
            //     plugin replying allow:false blocks it (mirrors the "auth" and
            //     "tool_before" veto hooks). With no join plugin registered the
            //     join proceeds.
            if state.plugins.has_hook("join") {
                let event = serde_json::json!({
                    "room": room, "old_room": conn.room, "sender": sender_ctx(conn),
                });
                if !state.plugins.veto_hook("join", &event).await {
                    warn!(
                        "Room join vetoed by plugin: user '{}' → '{}'",
                        conn.authed.as_ref().unwrap().username,
                        room
                    );
                    state.plugins.emit_server_event("room.denied", serde_json::json!({
                        "room": room, "username": conn.authed.as_ref().unwrap().username, "reason": "forbidden",
                    }));
                    let _ = send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::AuthFailure,
                        br#"{"error":"forbidden"}"#,
                    )
                    .await;
                    return Flow::Continue;
                }
            }

            let hub_id = conn.hub_id.expect("authed connection has hub id");
            if let Some(old_room) = state.hub.join_room(hub_id, &room) {
                if old_room != room {
                    // Tell the old room we left (we are no longer a member there).
                    state.hub.broadcast(
                        &old_room,
                        RouteMsg {
                            sender: conn.sid(),
                            msg_type: MessageType::PresenceUpdate,
                            payload: Bytes::from_static(b"LEAVE"),
                        },
                    );
                    // Tell the new room we arrived (excluding ourselves).
                    state.hub.broadcast_except(
                        &room,
                        RouteMsg {
                            sender: conn.sid(),
                            msg_type: MessageType::PresenceUpdate,
                            payload: Bytes::from_static(b"JOIN"),
                        },
                        hub_id,
                    );
                }
                conn.room = room.clone();
                info!(
                    "{} joined room '{}'",
                    conn.authed.as_ref().unwrap().username,
                    room
                );
                state.plugins.emit_server_event(
                    "room.joined",
                    serde_json::json!({
                        "room": room,
                        "username": conn.authed.as_ref().unwrap().username,
                    }),
                );
                if !send_direct(state, conn, ws_tx, MessageType::RoomJoined, room.as_bytes()).await
                {
                    return Flow::Close("write_error");
                }

                // --- Connection-state recovery -------------------------------
                // The client is now a room member, so nothing sent from here on is
                // lost. Replay the TextMessages this *session* missed while
                // disconnected. Best-effort: a single message at the reconnect
                // boundary may arrive both replayed and live, which clients dedup
                // by id. Recovery is node-local (same as presence).
                if recover {
                    let missed = state.hub.resume(conn.sid(), hub_id, &room);
                    if !missed.is_empty() {
                        info!(
                            "replaying {} missed message(s) to reconnecting session in '{}'",
                            missed.len(),
                            room
                        );
                    }
                    for m in missed {
                        if !send_direct(state, conn, ws_tx, m.msg_type, &m.payload).await {
                            return Flow::Close("write_error");
                        }
                    }
                }

                // --- Rich presence (presence-* rooms) ------------------------
                // A `presence-*` room maintains a member roster. Identity comes
                // only from the verified grant (`channel_grant`), never from
                // client input, so a client cannot claim to be another member.
                let new_member = if room.starts_with("presence-") {
                    conn.channel_grant.as_ref().and_then(|g| {
                        g.user_id.as_ref().map(|uid| crate::hub::PresenceMember {
                            user_id: uid.clone(),
                            user_info: g.user_info.clone(),
                        })
                    })
                } else {
                    None
                };
                // Retract membership in a previous presence room we've now left.
                if let Some(prev) = conn.presence_room.take() {
                    if prev == room {
                        conn.presence_room = Some(prev); // same room; keep it
                    } else if let Some(gone) = state.hub.presence_leave(&prev, conn.sid()) {
                        let evt = serde_json::json!({
                            "event": "presence:member_removed", "member": gone,
                        })
                        .to_string();
                        state.hub.broadcast(
                            &prev,
                            RouteMsg {
                                sender: conn.sid(),
                                msg_type: MessageType::PresenceUpdate,
                                payload: Bytes::from(evt.into_bytes()),
                            },
                        );
                    }
                }
                // Enter the new presence room: send the joiner the roster, and
                // (only for the user's first session) tell the room a member arrived.
                if let Some(member) = new_member {
                    let (roster, is_new) =
                        state.hub.presence_join(&room, conn.sid(), member.clone());
                    let here = serde_json::json!({
                        "event": "presence:here", "members": roster,
                    })
                    .to_string();
                    if !send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::PresenceUpdate,
                        here.as_bytes(),
                    )
                    .await
                    {
                        return Flow::Close("write_error");
                    }
                    if is_new {
                        let added = serde_json::json!({
                            "event": "presence:member_added", "member": member,
                        })
                        .to_string();
                        state.hub.broadcast_except(
                            &room,
                            RouteMsg {
                                sender: conn.sid(),
                                msg_type: MessageType::PresenceUpdate,
                                payload: Bytes::from(added.into_bytes()),
                            },
                            hub_id,
                        );
                    }
                    conn.presence_room = Some(room.clone());
                }
            }
            Flow::Continue
        }

        MessageType::ToolCall => {
            if conn.authed.is_none() {
                return unauthorized(state, conn, ws_tx).await;
            }
            let plaintext = match decrypt_in(conn, &packet) {
                Ok(p) => p,
                Err(_) => return Flow::Close("decrypt_failed"),
            };
            let (reply_type, reply_json) = execute_tool_call(state, conn, &plaintext).await;
            if !send_direct(state, conn, ws_tx, reply_type, reply_json.as_bytes()).await {
                return Flow::Close("write_error");
            }
            Flow::Continue
        }

        MessageType::ClientEvent => {
            if conn.authed.is_none() {
                return unauthorized(state, conn, ws_tx).await;
            }
            let plaintext = match decrypt_in(conn, &packet) {
                Ok(p) => p,
                Err(_) => return Flow::Close("decrypt_failed"),
            };
            // A "whisper": client-to-client, fanned out to the *other* members of
            // the room. Three rules, all matching Pusher/Reverb; a violation is
            // dropped silently (no echo, no error) so a client cannot probe rooms:
            //   1. only on a private/presence channel, never public,
            //   2. the event name must be `client-*`,
            //   3. a small payload (whispers are signals like "typing", not data).
            if plaintext.len() > 8192 {
                debug!("client event dropped: payload too large in '{}'", conn.room);
                return Flow::Continue;
            }
            if !state.cfg.room_requires_grant(&conn.room) {
                debug!(
                    "client event dropped: '{}' is not a private/presence room",
                    conn.room
                );
                return Flow::Continue;
            }
            #[derive(serde::Deserialize)]
            struct Whisper {
                event: String,
            }
            let named_ok = serde_json::from_slice::<Whisper>(&plaintext)
                .map(|w| w.event.starts_with("client-") && w.event.len() <= 128)
                .unwrap_or(false);
            if !named_ok {
                debug!(
                    "client event dropped: event name not client-* in '{}'",
                    conn.room
                );
                return Flow::Continue;
            }
            let hub_id = conn.hub_id.expect("authed connection has hub id");
            state.hub.broadcast_except(
                &conn.room,
                RouteMsg {
                    sender: conn.sid(),
                    msg_type: MessageType::ClientEvent,
                    payload: Bytes::from(plaintext),
                },
                hub_id,
            );
            Flow::Continue
        }

        t if is_routable(t) => {
            if conn.authed.is_none() {
                return unauthorized(state, conn, ws_tx).await;
            }
            let plaintext = match decrypt_in(conn, &packet) {
                Ok(p) => p,
                Err(_) => return Flow::Close("decrypt_failed"),
            };

            if t == MessageType::TextMessage {
                // Text fallback for clients without tool packets.
                if plaintext.starts_with(b"TOOL:") {
                    let (_, reply_json) = execute_tool_call(state, conn, &plaintext[5..]).await;
                    let reply = format!("TOOLRESULT:{reply_json}");
                    if !send_direct(
                        state,
                        conn,
                        ws_tx,
                        MessageType::TextMessage,
                        reply.as_bytes(),
                    )
                    .await
                    {
                        return Flow::Close("write_error");
                    }
                    return Flow::Continue;
                }
                // Moderation-style plugins may veto text messages.
                if state.plugins.has_hook("text") {
                    let event = serde_json::json!({
                        "room": conn.room,
                        "text": String::from_utf8_lossy(&plaintext),
                        "sender": sender_ctx(conn),
                    });
                    if !state.plugins.veto_hook("text", &event).await {
                        debug!("Text message blocked by plugin in '{}'", conn.room);
                        return Flow::Continue;
                    }
                }
            }

            if t == MessageType::FileInit && state.plugins.has_hook("file") {
                let meta: serde_json::Value =
                    serde_json::from_slice(&plaintext).unwrap_or(serde_json::Value::Null);
                let event = serde_json::json!({
                    "room": conn.room, "meta": meta, "sender": sender_ctx(conn),
                });
                if !state.plugins.veto_hook("file", &event).await {
                    debug!("File transfer blocked by plugin in '{}'", conn.room);
                    return Flow::Continue;
                }
            }

            if t == MessageType::FileComplete {
                state.plugins.emit_server_event(
                    "file.completed",
                    serde_json::json!({
                        "room": conn.room, "sender": sender_ctx(conn),
                    }),
                );
            }
            if t == MessageType::PresenceUpdate && state.plugins.has_hook("presence") {
                let event = serde_json::json!({
                    "room": conn.room,
                    "status": String::from_utf8_lossy(&plaintext),
                    "sender": sender_ctx(conn),
                });
                state.plugins.notify_hook("presence", &event).await;
            }

            let delivered = state.hub.broadcast(
                &conn.room,
                RouteMsg {
                    sender: conn.sid(),
                    msg_type: t,
                    payload: Bytes::from(plaintext),
                },
            );

            // Delivery ack: a client that set the RELIABLE flag on a TextMessage
            // gets a TextAck back confirming receipt and the local fan-out count,
            // correlated by the message's sequence. Opt-in — plain sends are
            // fire-and-forget as before. (Count is local; cross-node fan-out is
            // best-effort, same as the publish endpoint.)
            if t == MessageType::TextMessage && packet.header.flags.contains(PacketFlags::RELIABLE)
            {
                // seq as a string: a u64 sequence can exceed JS's safe-integer
                // range, and the client correlates by exact decimal string.
                let ack = serde_json::json!({
                    "seq": packet.header.sequence.to_string(),
                    "delivered": delivered,
                })
                .to_string();
                if !send_direct(state, conn, ws_tx, MessageType::TextAck, ack.as_bytes()).await {
                    return Flow::Close("write_error");
                }
            }
            Flow::Continue
        }

        other => {
            debug!("Ignoring unsupported packet type {:?} from {remote}", other);
            Flow::Continue
        }
    }
}

/// Caller identity attached to plugin hook/tool events.
fn sender_ctx(conn: &ConnState) -> serde_json::Value {
    serde_json::json!({
        "username": conn.authed.as_ref().map(|u| u.username.as_str()).unwrap_or("-"),
        "role": conn.authed.as_ref().map(|u| u.role.as_str()).unwrap_or("-"),
        "session": conn.sid().simple().to_string(),
    })
}

/// Parses and executes a ToolCall body; returns the reply packet type and
/// its JSON payload (contract: docs/spec/09-extensions.md).
async fn execute_tool_call(
    state: &Arc<AppState>,
    conn: &ConnState,
    body: &[u8],
) -> (MessageType, String) {
    #[derive(serde::Deserialize)]
    struct CallBody {
        #[serde(default)]
        id: String,
        tool: String,
        #[serde(default)]
        args: serde_json::Value,
    }

    let parsed: Result<CallBody, _> = serde_json::from_slice(body);
    let call = match parsed {
        Ok(c) if c.id.len() <= 64 && !c.tool.is_empty() => c,
        _ => {
            let reply = serde_json::json!({
                "id": "", "tool": "", "ok": false,
                "error": { "code": "tool_invalid_args", "message": "malformed ToolCall JSON" }
            });
            return (MessageType::ToolError, reply.to_string());
        }
    };

    let user = conn.authed.as_ref().expect("tool calls require auth");
    let caller = crate::plugins::CallerCtx {
        username: user.username.clone(),
        role: user.role.clone(),
        session: conn.sid().simple().to_string(),
        room: conn.room.clone(),
    };

    let args = if call.args.is_null() {
        serde_json::json!({})
    } else {
        call.args
    };
    match state.plugins.call_tool(&caller, &call.tool, args).await {
        Ok(result) => {
            let reply = serde_json::json!({
                "id": call.id, "tool": call.tool, "ok": true, "result": result
            });
            (MessageType::ToolResult, reply.to_string())
        }
        Err(e) => {
            let reply = serde_json::json!({
                "id": call.id, "tool": call.tool, "ok": false,
                "error": { "code": e.code, "message": e.message }
            });
            (MessageType::ToolError, reply.to_string())
        }
    }
}

/// Decrypt an inbound packet with the connection's session.
///
/// Encrypted packets are decrypted (which also enforces replay protection).
/// Plaintext packets normally pass through — but once a secure session exists
/// for this connection, accepting plaintext for a sensitive type would be an
/// encryption **downgrade** (Finding 4), so those are rejected (the caller maps
/// `Err` to a connection close). Pre-handshake and plaintext-only sessions
/// (`secure == None`) are unaffected, so anonymous / no-session flows keep
/// working.
fn decrypt_in(conn: &mut ConnState, packet: &Packet) -> Result<Vec<u8>, ()> {
    if packet.header.flags.contains(PacketFlags::ENCRYPTED) {
        match conn.secure.as_mut() {
            Some(secure) => secure.decrypt(packet).map_err(|_| ()),
            None => Err(()),
        }
    } else {
        if plaintext_downgrade_rejected(conn.secure.is_some(), packet.header.msg_type) {
            return Err(());
        }
        Ok(packet.payload.to_vec())
    }
}

/// True when a plaintext packet of type `t` must be refused because a secure
/// session already exists on the connection.
fn plaintext_downgrade_rejected(session_exists: bool, t: MessageType) -> bool {
    session_exists && requires_encryption(t)
}

/// Message types that MUST be encrypted once a secure session exists: auth,
/// room control, tool calls, and all room-routed text/data traffic — the
/// sensitive surface reachable through [`decrypt_in`].
fn requires_encryption(t: MessageType) -> bool {
    matches!(
        t,
        MessageType::AuthRequest
            | MessageType::JoinRoom
            | MessageType::ToolCall
            | MessageType::ClientEvent
    ) || is_routable(t)
}

async fn unauthorized(
    state: &Arc<AppState>,
    conn: &mut ConnState,
    ws_tx: &mut (impl SinkExt<Message> + Unpin),
) -> Flow {
    conn.preauth_violations += 1;
    let _ = send_direct(
        state,
        conn,
        ws_tx,
        MessageType::AuthFailure,
        br#"{"error":"not_authenticated"}"#,
    )
    .await;
    if conn.preauth_violations >= MAX_PREAUTH_VIOLATIONS {
        Flow::Close("preauth_flood")
    } else {
        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_caps_burst_then_refills() {
        let mut rl = RateLimiter::new(5);
        let t0 = Instant::now();
        // A burst up to capacity is allowed at a single instant.
        for i in 0..5 {
            assert!(
                rl.allow(t0),
                "message {i} within the burst budget should pass"
            );
        }
        // One more at the same instant is over the limit.
        assert!(!rl.allow(t0), "burst + 1 must be rejected");
        // After a full second, tokens refill and traffic flows again.
        assert!(
            rl.allow(t0 + Duration::from_secs(1)),
            "a token must be available after 1s"
        );
    }

    #[test]
    fn rate_limiter_zero_is_unlimited() {
        let mut rl = RateLimiter::new(0);
        let t0 = Instant::now();
        for _ in 0..10_000 {
            assert!(rl.allow(t0), "a rate of 0 disables limiting");
        }
    }

    #[test]
    fn connection_cap_acquire_and_release() {
        let counter = Arc::new(AtomicUsize::new(0));
        let g1 = try_acquire(&counter, 2).expect("slot 1");
        let _g2 = try_acquire(&counter, 2).expect("slot 2");
        // At the cap, further acquisitions are rejected and reserve nothing.
        assert!(
            try_acquire(&counter, 2).is_none(),
            "over the cap must be rejected"
        );
        assert_eq!(
            counter.load(Ordering::Acquire),
            2,
            "a rejected acquire leaks no slot"
        );
        // Releasing a slot frees capacity again.
        drop(g1);
        assert_eq!(counter.load(Ordering::Acquire), 1);
        assert!(
            try_acquire(&counter, 2).is_some(),
            "a freed slot can be reused"
        );
    }

    #[test]
    fn requires_encryption_covers_sensitive_types() {
        assert!(requires_encryption(MessageType::AuthRequest));
        assert!(requires_encryption(MessageType::JoinRoom));
        assert!(requires_encryption(MessageType::ToolCall));
        assert!(requires_encryption(MessageType::TextMessage));
        assert!(requires_encryption(MessageType::FileChunk));
        assert!(requires_encryption(MessageType::GameState));
        // Handshake + keepalive never pass through decrypt_in and may be plaintext.
        assert!(!requires_encryption(MessageType::HandshakeInit));
        assert!(!requires_encryption(MessageType::Ping));
    }

    #[test]
    fn plaintext_rejected_only_after_session_established() {
        // No session yet: a plaintext AuthRequest is the normal pre-handshake flow.
        assert!(!plaintext_downgrade_rejected(
            false,
            MessageType::AuthRequest
        ));
        // Session exists: plaintext for sensitive types is a downgrade → rejected.
        assert!(plaintext_downgrade_rejected(true, MessageType::AuthRequest));
        assert!(plaintext_downgrade_rejected(true, MessageType::TextMessage));
        assert!(plaintext_downgrade_rejected(true, MessageType::JoinRoom));
        // Even with a session, a type never handled via decrypt_in is not our concern here.
        assert!(!plaintext_downgrade_rejected(true, MessageType::Ping));
    }
}
