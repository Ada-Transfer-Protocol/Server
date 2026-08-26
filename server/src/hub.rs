use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use bytes::Bytes;
use dashmap::DashMap;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use adatp_core::MessageType;

pub type ConnId = u64;

/// How many recent `TextMessage`s a room keeps for reconnect replay.
const BACKLOG_MAX_MSGS: usize = 256;
/// How long a dropped session's replay cursor is remembered.
const RECOVERY_TTL_MS: u64 = 120_000;

/// Per-room replay log for connection-state recovery. Offsets are **1-based**
/// (0 means "nothing delivered yet") and monotonic *within this node* — recovery
/// is node-local, like presence.
#[derive(Default)]
struct RoomLog {
    next_offset: u64,
    backlog: VecDeque<(u64, RouteMsg)>,
}

/// A dropped connection's replay cursor, kept for `RECOVERY_TTL_MS` so a client
/// reconnecting with the same session id can replay the `TextMessage`s it missed.
struct Recovery {
    room: String,
    last_offset: u64,
    saved_at_ms: u64,
}

/// A message routed between connections. Payload is always plaintext here;
/// each receiving connection encrypts it with its own session keys (or sends
/// it as-is for plaintext sessions).
#[derive(Clone, Debug)]
pub struct RouteMsg {
    /// Session id of the originating connection — stamped into the header so
    /// receivers can identify the sender.
    pub sender: Uuid,
    pub msg_type: MessageType,
    pub payload: Bytes,
}

/// Events delivered to a connection's outbound queue.
#[derive(Clone, Debug)]
pub enum OutEvent {
    Route(RouteMsg),
    /// Ask the connection to close gracefully (admin drain / shutdown).
    Shutdown,
}

// Fields feed the admin control plane's connection views.
#[allow(dead_code)]
pub struct ConnEntry {
    pub id: ConnId,
    pub session_id: Uuid,
    pub username: String,
    pub role: String,
    pub room: String,
    pub remote: String,
    pub connected_at_ms: u64,
    pub tx: mpsc::Sender<OutEvent>,
    /// Highest room `TextMessage` offset delivered to this connection (0 = none).
    /// Read at disconnect to seed the session's recovery cursor. Atomic so the
    /// hot delivery path can bump it under a shared (read) lock.
    pub last_offset: AtomicU64,
}

/// Serializable view of a connection for the admin plane.
#[allow(dead_code)]
#[derive(Serialize, Clone)]
pub struct ConnView {
    pub id: ConnId,
    pub session_id: String,
    pub username: String,
    pub role: String,
    pub room: String,
    pub remote: String,
    pub connected_at_ms: u64,
}

#[derive(Serialize, Clone)]
pub struct RoomView {
    pub name: String,
    pub members: usize,
}

/// A presence member as clients see it: a stable id and app-supplied metadata,
/// both taken from the *signed* channel-auth grant (never from client JSON).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PresenceMember {
    pub user_id: String,
    pub user_info: Value,
}

/// Collapse a room's per-session members to one entry per `user_id` (a user with
/// two tabs is one member), ordered by `user_id` for a stable roster.
fn dedup_roster(members: &HashMap<Uuid, PresenceMember>) -> Vec<PresenceMember> {
    let mut seen = HashSet::new();
    let mut out: Vec<PresenceMember> = members
        .values()
        .filter(|m| seen.insert(m.user_id.clone()))
        .cloned()
        .collect();
    out.sort_by(|a, b| a.user_id.cmp(&b.user_id));
    out
}

/// Central connection registry and room router.
///
/// Locking discipline: `conns` and `rooms` are independent DashMaps; no method
/// holds a reference into one while locking the other shard of the same map,
/// and broadcast snapshots member lists before sending.
pub struct Hub {
    next_id: AtomicU64,
    conns: DashMap<ConnId, ConnEntry>,
    rooms: DashMap<String, HashSet<ConnId>>,
    /// Messages dropped because a receiver's queue was full (backpressure).
    pub dropped_msgs: AtomicU64,
    /// When a multi-node backplane is active, room broadcasts are also forwarded
    /// here (room, msg, exclude_session) so the backplane can publish them to
    /// other nodes. Unset on a single-node deployment — then `broadcast` is
    /// purely in-process.
    publish_tx: OnceLock<mpsc::Sender<(String, RouteMsg, Option<Uuid>)>>,
    /// Presence rosters: room → (session id → member). Only presence-`* rooms
    /// have an entry. Node-local: cross-node presence is out of scope for v1
    /// (documented), so a roster reflects members on *this* node.
    presence: DashMap<String, HashMap<Uuid, PresenceMember>>,
    /// Per-room replay logs for connection-state recovery (TextMessages only).
    room_logs: DashMap<String, RoomLog>,
    /// Dropped sessions' replay cursors, keyed by session id, TTL-expired.
    recovery: DashMap<Uuid, Recovery>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            conns: DashMap::new(),
            rooms: DashMap::new(),
            dropped_msgs: AtomicU64::new(0),
            publish_tx: OnceLock::new(),
            presence: DashMap::new(),
            room_logs: DashMap::new(),
            recovery: DashMap::new(),
        }
    }

    /// Wire the multi-node backplane's publish channel. Called once at startup
    /// when `ADATP_BACKPLANE_URL` is set; a no-op if already set.
    pub fn set_publisher(&self, tx: mpsc::Sender<(String, RouteMsg, Option<Uuid>)>) {
        let _ = self.publish_tx.set(tx);
    }

    /// Forward a broadcast to the backplane (other nodes), if one is active.
    /// `exclude_session` propagates so remote nodes also skip that connection.
    fn forward_to_backplane(&self, room: &str, msg: &RouteMsg, exclude_session: Option<Uuid>) {
        if let Some(tx) = self.publish_tx.get() {
            // Non-blocking: if the backplane queue is full, drop (counted) rather
            // than stall the hot path. Cross-node delivery is best-effort, same
            // as the local queues.
            if tx
                .try_send((room.to_string(), msg.clone(), exclude_session))
                .is_err()
            {
                self.dropped_msgs.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Assign a room offset and append to the replay backlog — but only for
    /// `TextMessage`, the one type reconnect-recovery replays. Ephemeral signals
    /// (presence, typing, whispers) and streaming media are never backlogged.
    /// Returns the 1-based offset when recorded, else `None`.
    fn record_and_offset(&self, room: &str, msg: &RouteMsg) -> Option<u64> {
        if msg.msg_type != MessageType::TextMessage {
            return None;
        }
        let mut log = self.room_logs.entry(room.to_string()).or_default();
        log.next_offset += 1;
        let offset = log.next_offset;
        log.backlog.push_back((offset, msg.clone()));
        while log.backlog.len() > BACKLOG_MAX_MSGS {
            log.backlog.pop_front();
        }
        Some(offset)
    }

    /// Deliver `msg` to local members of `room`, optionally skipping the
    /// connection with `except` (ConnId) and/or `exclude_session` (session id).
    /// Returns the number of local deliveries. Never touches the backplane.
    fn deliver_local(
        &self,
        room: &str,
        msg: &RouteMsg,
        except: Option<ConnId>,
        exclude_session: Option<Uuid>,
    ) -> usize {
        // Record for reconnect replay (TextMessage only) and advance each
        // recipient's cursor so a later disconnect knows what it had received.
        let offset = self.record_and_offset(room, msg);
        let member_ids: Vec<ConnId> = match self.rooms.get(room) {
            Some(members) => members
                .iter()
                .copied()
                .filter(|id| Some(*id) != except)
                .collect(),
            None => return 0,
        };
        let mut delivered = 0usize;
        for id in member_ids {
            if let Some(entry) = self.conns.get(&id) {
                if let Some(ex) = exclude_session {
                    if entry.session_id == ex {
                        continue;
                    }
                }
                if entry.tx.try_send(OutEvent::Route(msg.clone())).is_ok() {
                    delivered += 1;
                    if let Some(o) = offset {
                        entry.last_offset.fetch_max(o, Ordering::Relaxed);
                    }
                } else {
                    self.dropped_msgs.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        delivered
    }

    /// Register an authenticated connection and place it in `room`.
    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &self,
        session_id: Uuid,
        username: String,
        role: String,
        room: String,
        remote: String,
        tx: mpsc::Sender<OutEvent>,
    ) -> ConnId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = ConnEntry {
            id,
            session_id,
            username,
            role,
            room: room.clone(),
            remote,
            connected_at_ms: now_ms(),
            tx,
            last_offset: AtomicU64::new(0),
        };
        self.conns.insert(id, entry);
        self.rooms.entry(room).or_default().insert(id);
        id
    }

    /// Move a connection to another room. Returns the previous room.
    pub fn join_room(&self, id: ConnId, new_room: &str) -> Option<String> {
        let old_room = {
            let mut entry = self.conns.get_mut(&id)?;
            let old = entry.room.clone();
            entry.room = new_room.to_string();
            old
        };
        if let Some(mut members) = self.rooms.get_mut(&old_room) {
            members.remove(&id);
        }
        self.rooms.retain(|_, v| !v.is_empty());
        self.rooms
            .entry(new_room.to_string())
            .or_default()
            .insert(id);
        Some(old_room)
    }

    /// Remove a connection entirely. Returns (room, session_id) so the caller
    /// can announce the departure.
    pub fn unregister(&self, id: ConnId) -> Option<(String, Uuid)> {
        let (_, entry) = self.conns.remove(&id)?;
        if let Some(mut members) = self.rooms.get_mut(&entry.room) {
            members.remove(&id);
        }
        self.rooms.retain(|_, v| !v.is_empty());
        // Remember where this session was so a reconnect can replay what it
        // missed. Kept for RECOVERY_TTL_MS; expired cursors are pruned lazily.
        let now = now_ms();
        if self.recovery.len() > 4096 {
            self.recovery
                .retain(|_, r| now.saturating_sub(r.saved_at_ms) < RECOVERY_TTL_MS);
        }
        self.recovery.insert(
            entry.session_id,
            Recovery {
                room: entry.room.clone(),
                last_offset: entry.last_offset.load(Ordering::Relaxed),
                saved_at_ms: now,
            },
        );
        Some((entry.room, entry.session_id))
    }

    /// A reconnecting client's replay: the backlogged `TextMessage`s for `room`
    /// that this session had not yet received when it dropped. Consumes the
    /// recovery cursor (one-shot) and seeds the new connection's cursor to the
    /// room's current high-water mark so live delivery continues seamlessly.
    /// Returns an empty vector when there is nothing to recover (no/expired
    /// cursor, a different room, or no missed messages).
    pub fn resume(&self, session_id: Uuid, new_conn: ConnId, room: &str) -> Vec<RouteMsg> {
        let cursor = match self.recovery.remove(&session_id) {
            Some((_, r)) => r,
            None => return Vec::new(),
        };
        if cursor.room != room || now_ms().saturating_sub(cursor.saved_at_ms) >= RECOVERY_TTL_MS {
            return Vec::new();
        }
        let (missed, high_water) = match self.room_logs.get(room) {
            Some(log) => (
                log.backlog
                    .iter()
                    .filter(|(o, _)| *o > cursor.last_offset)
                    .map(|(_, m)| m.clone())
                    .collect::<Vec<_>>(),
                log.next_offset,
            ),
            None => (Vec::new(), 0),
        };
        // Catch the new connection up so subsequent live messages (offset >
        // high_water) advance its cursor from the right place.
        if let Some(entry) = self.conns.get(&new_conn) {
            entry.last_offset.fetch_max(high_water, Ordering::Relaxed);
        }
        missed
    }

    /// Deliver `msg` to every member of `room`, including the sender
    /// (clients rely on their own echo, e.g. for RTT measurement), and forward
    /// it to the backplane so members on other nodes receive it too.
    /// Slow consumers whose queues are full lose the message (counted).
    pub fn broadcast(&self, room: &str, msg: RouteMsg) {
        self.deliver_local(room, &msg, None, None);
        self.forward_to_backplane(room, &msg, None);
    }

    /// Like `broadcast`, but skips `except` locally (e.g. a join announcement the
    /// joiner should not receive about itself). The backplane forward carries no
    /// exception — on other nodes the excepted connection does not exist, so all
    /// their room members receive it.
    pub fn broadcast_except(&self, room: &str, msg: RouteMsg, except: ConnId) {
        self.deliver_local(room, &msg, Some(except), None);
        self.forward_to_backplane(room, &msg, None);
    }

    /// App-server publish (via `POST /publish`): fan out to `room`, skipping any
    /// connection whose session id equals `exclude_session` (Laravel's
    /// `->toOthers()`), across the whole cluster. Returns the LOCAL delivery
    /// count (remote counts are not collected synchronously).
    pub fn broadcast_publish(
        &self,
        room: &str,
        msg: RouteMsg,
        exclude_session: Option<Uuid>,
    ) -> usize {
        let n = self.deliver_local(room, &msg, None, exclude_session);
        self.forward_to_backplane(room, &msg, exclude_session);
        n
    }

    /// Deliver a message that arrived **from the backplane** to local members
    /// only — never re-published, so there is no cross-node loop. `exclude_session`
    /// (carried in the backplane envelope) is honoured so `->toOthers()` holds
    /// even when the excluded connection is on this node.
    pub fn broadcast_local(&self, room: &str, msg: RouteMsg, exclude_session: Option<Uuid>) {
        self.deliver_local(room, &msg, None, exclude_session);
    }

    /// Add a presence member (keyed by session). Returns the room's full roster
    /// after the join (deduped by `user_id`, this member included) and whether
    /// this is the *first* session for that `user_id` — i.e. whether the room
    /// should be told `member_added`. A second tab for the same user joins the
    /// roster silently.
    pub fn presence_join(
        &self,
        room: &str,
        session: Uuid,
        member: PresenceMember,
    ) -> (Vec<PresenceMember>, bool) {
        let mut entry = self.presence.entry(room.to_string()).or_default();
        let is_new_user = !entry.values().any(|m| m.user_id == member.user_id);
        entry.insert(session, member);
        (dedup_roster(&entry), is_new_user)
    }

    /// Remove a presence session. Returns `Some(member)` when that was the
    /// *last* session for the `user_id` — i.e. the room should be told
    /// `member_removed` — otherwise `None` (the user still has another tab).
    pub fn presence_leave(&self, room: &str, session: Uuid) -> Option<PresenceMember> {
        let mut removed_last = None;
        let mut now_empty = false;
        if let Some(mut entry) = self.presence.get_mut(room) {
            if let Some(member) = entry.remove(&session) {
                let still_present = entry.values().any(|m| m.user_id == member.user_id);
                if !still_present {
                    removed_last = Some(member);
                }
            }
            now_empty = entry.is_empty();
        }
        // Drop the shard guard (scope above) before removing the room key, so we
        // never hold a get_mut borrow while locking the same map for remove.
        if now_empty {
            self.presence.remove(room);
        }
        removed_last
    }

    pub fn connection_count(&self) -> usize {
        self.conns.len()
    }

    #[allow(dead_code)] // consumed by /admin/v1
    pub fn list_connections(&self) -> Vec<ConnView> {
        self.conns
            .iter()
            .map(|e| ConnView {
                id: e.id,
                session_id: e.session_id.simple().to_string(),
                username: e.username.clone(),
                role: e.role.clone(),
                room: e.room.clone(),
                remote: e.remote.clone(),
                connected_at_ms: e.connected_at_ms,
            })
            .collect()
    }

    pub fn list_rooms(&self) -> Vec<RoomView> {
        self.rooms
            .iter()
            .map(|e| RoomView {
                name: e.key().clone(),
                members: e.value().len(),
            })
            .collect()
    }

    /// Ask every connection to close (graceful shutdown / drain).
    pub fn shutdown_all(&self) {
        for entry in self.conns.iter() {
            let _ = entry.tx.try_send(OutEvent::Shutdown);
        }
    }

    /// Ask one connection to close. Returns false if unknown.
    #[allow(dead_code)] // consumed by /admin/v1
    pub fn kick(&self, id: ConnId) -> bool {
        match self.conns.get(&id) {
            Some(entry) => entry.tx.try_send(OutEvent::Shutdown).is_ok(),
            None => false,
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn member(user: &str, name: &str) -> PresenceMember {
        PresenceMember {
            user_id: user.to_string(),
            user_info: json!({ "name": name }),
        }
    }

    #[test]
    fn first_session_is_new_and_appears_in_roster() {
        let hub = Hub::new();
        let (roster, is_new) =
            hub.presence_join("presence-chat", Uuid::from_u128(1), member("u1", "Ada"));
        assert!(
            is_new,
            "the first session for a user announces member_added"
        );
        assert_eq!(roster, vec![member("u1", "Ada")]);
    }

    #[test]
    fn second_tab_same_user_is_silent_and_deduped() {
        let hub = Hub::new();
        hub.presence_join("presence-chat", Uuid::from_u128(1), member("u1", "Ada"));
        // Same user_id, different session (a second browser tab).
        let (roster, is_new) =
            hub.presence_join("presence-chat", Uuid::from_u128(2), member("u1", "Ada"));
        assert!(!is_new, "a second tab must not re-announce the member");
        assert_eq!(roster.len(), 1, "the roster dedups by user_id");
    }

    #[test]
    fn distinct_users_both_announce_and_roster_is_sorted() {
        let hub = Hub::new();
        hub.presence_join("presence-chat", Uuid::from_u128(1), member("u2", "Bee"));
        let (roster, is_new) =
            hub.presence_join("presence-chat", Uuid::from_u128(2), member("u1", "Ada"));
        assert!(is_new);
        // Sorted by user_id, so u1 before u2 regardless of join order.
        assert_eq!(roster, vec![member("u1", "Ada"), member("u2", "Bee")]);
    }

    #[test]
    fn leave_reports_last_session_only() {
        let hub = Hub::new();
        hub.presence_join("presence-chat", Uuid::from_u128(1), member("u1", "Ada"));
        hub.presence_join("presence-chat", Uuid::from_u128(2), member("u1", "Ada"));
        // First tab leaves: user still present via the other tab → no member_removed.
        assert_eq!(
            hub.presence_leave("presence-chat", Uuid::from_u128(1)),
            None
        );
        // Last tab leaves: now the user is gone → announce member_removed.
        assert_eq!(
            hub.presence_leave("presence-chat", Uuid::from_u128(2)),
            Some(member("u1", "Ada"))
        );
    }

    fn text(sender: Uuid, s: &str) -> RouteMsg {
        RouteMsg {
            sender,
            msg_type: MessageType::TextMessage,
            payload: Bytes::from(s.to_string()),
        }
    }

    #[test]
    fn recovery_replays_only_the_missed_text_messages() {
        let hub = Hub::new();
        let sa = Uuid::from_u128(100);
        let (tx, _rx) = mpsc::channel(64);
        let a = hub.register(sa, "u".into(), "r".into(), "chat".into(), "ip".into(), tx);
        hub.broadcast("chat", text(sa, "m1"));
        hub.broadcast("chat", text(sa, "m2"));
        // A drops after receiving m1, m2 (cursor at offset 2).
        hub.unregister(a);
        // Two more messages arrive while A is away.
        hub.broadcast("chat", text(sa, "m3"));
        hub.broadcast("chat", text(sa, "m4"));
        // A reconnects with the same session id and recovers.
        let (tx2, _rx2) = mpsc::channel(64);
        let a2 = hub.register(sa, "u".into(), "r".into(), "chat".into(), "ip".into(), tx2);
        let missed: Vec<String> = hub
            .resume(sa, a2, "chat")
            .iter()
            .map(|m| String::from_utf8_lossy(&m.payload).into_owned())
            .collect();
        assert_eq!(missed, vec!["m3", "m4"]);
        // Recovery is one-shot: a second resume finds nothing.
        assert!(hub.resume(sa, a2, "chat").is_empty());
    }

    #[test]
    fn recovery_ignores_non_text_unknown_session_and_wrong_room() {
        let hub = Hub::new();
        let s = Uuid::from_u128(7);
        let (tx, _rx) = mpsc::channel(64);
        let c = hub.register(s, "u".into(), "r".into(), "chat".into(), "ip".into(), tx);
        // A presence signal is not part of the replay log.
        hub.broadcast(
            "chat",
            RouteMsg {
                sender: s,
                msg_type: MessageType::PresenceUpdate,
                payload: Bytes::from_static(b"JOIN"),
            },
        );
        hub.unregister(c);
        let (tx2, _rx2) = mpsc::channel(64);
        let c2 = hub.register(s, "u".into(), "r".into(), "chat".into(), "ip".into(), tx2);
        // Nothing recoverable: no TextMessage was ever logged.
        assert!(hub.resume(s, c2, "chat").is_empty());
        // Unknown session and wrong-room cursors recover nothing.
        assert!(hub.resume(Uuid::from_u128(999), c2, "chat").is_empty());
    }

    #[test]
    fn empty_room_is_dropped_and_unknown_leave_is_none() {
        let hub = Hub::new();
        hub.presence_join("presence-chat", Uuid::from_u128(1), member("u1", "Ada"));
        assert_eq!(
            hub.presence_leave("presence-chat", Uuid::from_u128(1)),
            Some(member("u1", "Ada"))
        );
        // The room key is gone once the last member leaves.
        assert!(hub.presence.get("presence-chat").is_none());
        // Leaving a room with no roster is a harmless no-op.
        assert_eq!(
            hub.presence_leave("presence-chat", Uuid::from_u128(9)),
            None
        );
    }
}
