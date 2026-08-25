use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use serde::Serialize;

use crate::hub::Hub;
use crate::metrics::Metrics;

const WINDOW: usize = 60; // one sample per second, one minute of history

#[derive(Serialize, Clone, Copy, Debug)]
pub struct LoadSample {
    pub at_ms: u64,
    pub connections: usize,
    pub rx_bytes_per_s: u64,
    pub tx_bytes_per_s: u64,
}

/// Samples throughput once per second so the admin plane can graph load
/// without the hot path doing any extra work.
pub struct LoadTracker {
    samples: Mutex<VecDeque<LoadSample>>,
}

impl LoadTracker {
    pub fn start(metrics: Arc<Metrics>, hub: Arc<Hub>) -> Arc<Self> {
        let tracker = Arc::new(Self {
            samples: Mutex::new(VecDeque::with_capacity(WINDOW)),
        });

        let t = tracker.clone();
        tokio::spawn(async move {
            let mut last_rx = 0u64;
            let mut last_tx = 0u64;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let snap = metrics.snapshot();
                let sample = LoadSample {
                    at_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                    connections: hub.connection_count(),
                    rx_bytes_per_s: snap.total_bytes_received.saturating_sub(last_rx),
                    tx_bytes_per_s: snap.total_bytes_sent.saturating_sub(last_tx),
                };
                last_rx = snap.total_bytes_received;
                last_tx = snap.total_bytes_sent;

                let mut samples = t.samples.lock().unwrap();
                if samples.len() >= WINDOW {
                    samples.pop_front();
                }
                samples.push_back(sample);
            }
        });

        tracker
    }

    pub fn series(&self) -> Vec<LoadSample> {
        self.samples.lock().unwrap().iter().copied().collect()
    }

    pub fn current(&self) -> Option<LoadSample> {
        self.samples.lock().unwrap().back().copied()
    }
}
