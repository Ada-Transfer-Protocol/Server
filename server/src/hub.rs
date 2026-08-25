use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use adatp_core::MessageType;

pub type ConnId = u64;

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
}

impl Hub {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            conns: DashMap::new(),
            rooms: DashMap::new(),
            dropped_msgs: AtomicU64::new(0),
        }
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
        Some((entry.room, entry.session_id))
    }

    /// Deliver `msg` to every member of `room`, including the sender
    /// (clients rely on their own echo, e.g. for RTT measurement).
    /// Slow consumers whose queues are full lose the message (counted).
    pub fn broadcast(&self, room: &str, msg: RouteMsg) {
        let member_ids: Vec<ConnId> = match self.rooms.get(room) {
            Some(members) => members.iter().copied().collect(),
            None => return,
        };
        for id in member_ids {
            if let Some(entry) = self.conns.get(&id) {
                if entry.tx.try_send(OutEvent::Route(msg.clone())).is_err() {
                    self.dropped_msgs.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Like `broadcast`, but skips `except` (e.g. join announcements that the
    /// joiner should not receive about itself).
    pub fn broadcast_except(&self, room: &str, msg: RouteMsg, except: ConnId) {
        let member_ids: Vec<ConnId> = match self.rooms.get(room) {
            Some(members) => members.iter().copied().filter(|id| *id != except).collect(),
            None => return,
        };
        for id in member_ids {
            if let Some(entry) = self.conns.get(&id) {
                if entry.tx.try_send(OutEvent::Route(msg.clone())).is_err() {
                    self.dropped_msgs.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
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
