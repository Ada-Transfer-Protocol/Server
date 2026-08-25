use std::collections::VecDeque;
use std::sync::Mutex;

use log::{Level, Metadata, Record};
use serde::Serialize;
use tokio::sync::broadcast;

const RING_CAPACITY: usize = 500;

#[derive(Serialize, Clone, Debug)]
pub struct LogLine {
    pub at_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Logger that mirrors every record to stderr (like env_logger) while also
/// keeping a ring buffer and fanning lines out to SSE subscribers — the
/// admin UI consumes the same stream operators see in the terminal.
pub struct BufLogger {
    max_level: Level,
    ring: Mutex<VecDeque<LogLine>>,
    tx: broadcast::Sender<LogLine>,
}

impl BufLogger {
    /// Installs the logger. Respects RUST_LOG=error|warn|info|debug|trace
    /// (simple level filter, no per-module directives).
    pub fn init() -> &'static BufLogger {
        let max_level = match std::env::var("RUST_LOG").as_deref() {
            Ok("error") => Level::Error,
            Ok("warn") => Level::Warn,
            Ok("debug") => Level::Debug,
            Ok("trace") => Level::Trace,
            _ => Level::Info,
        };
        let (tx, _) = broadcast::channel(512);
        let logger: &'static BufLogger = Box::leak(Box::new(BufLogger {
            max_level,
            ring: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
            tx,
        }));
        log::set_logger(logger).expect("logger already set");
        log::set_max_level(max_level.to_level_filter());
        logger
    }

    pub fn recent(&self, n: usize) -> Vec<LogLine> {
        let ring = self.ring.lock().unwrap();
        ring.iter().rev().take(n).rev().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl log::Log for BufLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Skip noisy third-party targets at debug/trace.
        let target = record.target();
        let line = LogLine {
            at_ms: now_ms(),
            level: record.level().to_string(),
            target: target.to_string(),
            message: record.args().to_string(),
        };

        eprintln!(
            "[{} {} {}] {}",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            line.level,
            line.target,
            line.message
        );

        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() >= RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line.clone());
        }
        let _ = self.tx.send(line);
    }

    fn flush(&self) {}
}
