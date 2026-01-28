use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct RoomStat {
    pub name: String,
    pub user_count: usize,
}

pub struct Metrics {
    pub active_connections: AtomicUsize,
    pub total_bytes_rx: AtomicU64,
    pub total_bytes_tx: AtomicU64,
    pub start_time: Instant,
    // Map<RoomName, UserCount> - This is simplified, real count logic is in connection manager
    // But we can mirror it here or query connection manager.
    // For simplicity, let's just track rooms in connection manager and expose via API.
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            active_connections: AtomicUsize::new(0),
            total_bytes_rx: AtomicU64::new(0),
            total_bytes_tx: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn inc_connection(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_connection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn add_rx(&self, size: u64) {
        self.total_bytes_rx.fetch_add(size, Ordering::Relaxed);
    }

    pub fn add_tx(&self, size: u64) {
        self.total_bytes_tx.fetch_add(size, Ordering::Relaxed);
    }
}

// Snapshot struct for JSON response
#[derive(Serialize)]
pub struct MetricsSnapshot {
    pub uptime_seconds: u64,
    pub active_connections: usize,
    pub total_bytes_received: u64,
    pub total_bytes_sent: u64,
    pub avg_rx_speed_bps: u64, // Bytes per second (Avg over total uptime)
}

impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        let uptime = self.start_time.elapsed().as_secs();
        let safe_uptime = if uptime == 0 { 1 } else { uptime };
        let rx = self.total_bytes_rx.load(Ordering::Relaxed);
        
        MetricsSnapshot {
            uptime_seconds: uptime,
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_bytes_received: rx,
            total_bytes_sent: self.total_bytes_tx.load(Ordering::Relaxed),
            avg_rx_speed_bps: rx / safe_uptime,
        }
    }
}
