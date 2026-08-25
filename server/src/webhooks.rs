use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use hmac::{Hmac, Mac};
use log::warn;
use serde::Serialize;
use serde_json::json;
use sha2::Sha256;
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;

use crate::db::{DbManager, WebhookRow};
use crate::plugins::{PluginEvent, PluginManager};

const MAX_ATTEMPTS: u32 = 5;
const BREAKER_THRESHOLD: u32 = 5;
const BREAKER_OPEN_MS: u64 = 60_000;
const QUEUE_CAPACITY: usize = 1024;
const AUDIT_CAPACITY: usize = 256;
const DELIVERY_TIMEOUT_SECS: u64 = 10;

#[derive(Clone, Debug)]
struct Delivery {
    endpoint_id: String,
    event: String,
    body: String,
    delivery_id: String,
    attempt: u32,
}

#[derive(Default)]
struct EndpointStats {
    delivered: AtomicU64,
    failed: AtomicU64,
    skipped_breaker: AtomicU64,
}

#[derive(Default)]
struct BreakerState {
    consecutive_failures: u32,
    open_until_ms: u64,
}

/// One line of the delivery audit log (admin-visible).
#[derive(Serialize, Clone)]
pub struct AuditEntry {
    pub at_ms: u64,
    pub endpoint_id: String,
    pub event: String,
    pub outcome: String, // delivered | retrying | failed | skipped_breaker | blocked_ssrf
    pub status: Option<u16>,
    pub attempt: u32,
}

/// Admin-facing view of one endpoint (secret redacted).
#[derive(Serialize)]
pub struct WebhookView {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub description: Option<String>,
    pub created_at: String,
    pub delivered: u64,
    pub failed: u64,
    pub skipped_breaker: u64,
    pub breaker_open: bool,
}

/// Asynchronous webhook dispatcher: consumes the plugin/server event bus and
/// POSTs signed JSON envelopes to every matching endpoint, with retries and
/// a per-endpoint circuit breaker. Delivery never blocks the data plane.
pub struct WebhookManager {
    db: Arc<DbManager>,
    endpoints: RwLock<Vec<WebhookRow>>,
    queue_tx: mpsc::Sender<Delivery>,
    breaker: DashMap<String, BreakerState>,
    stats: DashMap<String, Arc<EndpointStats>>,
    audit: Mutex<VecDeque<AuditEntry>>,
    pub dropped_events: AtomicU64,
    allow_private: bool,
    http: reqwest::Client,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// hex(HMAC-SHA256(secret, body))
fn sign(secret: &str, body: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Does `filter` match `event`? Supported: exact, `prefix.*`, `*`.
fn filter_matches(filter: &str, event: &str) -> bool {
    if filter == "*" {
        return true;
    }
    if let Some(prefix) = filter.strip_suffix(".*") {
        return event.starts_with(prefix) && event.len() > prefix.len();
    }
    filter == event
}

/// SSRF guard: http(s) only, no credentials in URL, and the host must not
/// resolve to loopback / private / link-local space (unless explicitly
/// allowed via ADATP_WEBHOOK_ALLOW_PRIVATE=1 for local development).
async fn ssrf_check(raw_url: &str, allow_private: bool) -> Result<(), String> {
    let parsed = url::Url::parse(raw_url).map_err(|e| format!("invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        s => return Err(format!("scheme '{s}' not allowed")),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("credentials in webhook URLs are not allowed".into());
    }
    let host = parsed.host_str().ok_or("URL has no host")?.to_string();
    if allow_private {
        return Ok(());
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| format!("DNS resolution failed: {e}"))?;
    for addr in addrs {
        let ip = addr.ip();
        let private = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
                    || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
            }
        };
        if private {
            return Err(format!("host resolves to non-public address {ip}"));
        }
    }
    Ok(())
}

impl WebhookManager {
    pub async fn start(db: Arc<DbManager>, plugins: Arc<PluginManager>) -> Arc<Self> {
        let (queue_tx, queue_rx) = mpsc::channel::<Delivery>(QUEUE_CAPACITY);
        let allow_private = std::env::var("ADATP_WEBHOOK_ALLOW_PRIVATE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let mgr = Arc::new(Self {
            db,
            endpoints: RwLock::new(Vec::new()),
            queue_tx,
            breaker: DashMap::new(),
            stats: DashMap::new(),
            audit: Mutex::new(VecDeque::with_capacity(AUDIT_CAPACITY)),
            dropped_events: AtomicU64::new(0),
            allow_private,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(DELIVERY_TIMEOUT_SECS))
                .build()
                .expect("reqwest client"),
        });

        mgr.refresh().await;
        if allow_private {
            warn!("Webhooks: ADATP_WEBHOOK_ALLOW_PRIVATE=1 — SSRF guards relaxed (dev mode)");
        }

        // Event pump: bus → queue.
        let pump = mgr.clone();
        let mut events = plugins.subscribe_events();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => pump.dispatch(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        pump.dropped_events.fetch_add(n, Ordering::Relaxed);
                        warn!("Webhook pump lagged; {n} event(s) dropped");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Delivery workers.
        let worker_rx = Arc::new(Mutex::new(queue_rx));
        for _ in 0..2 {
            let mgr_w = mgr.clone();
            let rx = worker_rx.clone();
            tokio::spawn(async move {
                loop {
                    let delivery = { rx.lock().await.recv().await };
                    match delivery {
                        Some(d) => mgr_w.deliver(d).await,
                        None => break,
                    }
                }
            });
        }

        // Periodic endpoint refresh (admin CRUD also triggers refresh()).
        let refresher = mgr.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                refresher.refresh().await;
            }
        });

        mgr
    }

    /// Reloads the endpoint list from the database.
    pub async fn refresh(&self) {
        match self.db.webhook_list().await {
            Ok(rows) => {
                for row in &rows {
                    self.stats.entry(row.id.clone()).or_default();
                }
                *self.endpoints.write().await = rows;
            }
            Err(e) => warn!("Webhook endpoint refresh failed: {e}"),
        }
    }

    /// Fans one event out to every matching active endpoint.
    async fn dispatch(&self, event: PluginEvent) {
        let endpoints = self.endpoints.read().await;
        if endpoints.is_empty() {
            return;
        }
        for ep in endpoints.iter().filter(|e| e.is_active) {
            let filters: Vec<String> = serde_json::from_str(&ep.events).unwrap_or_default();
            if !filters.iter().any(|f| filter_matches(f, &event.event)) {
                continue;
            }
            let delivery_id = Uuid::new_v4().to_string();
            let body = json!({
                "id": delivery_id,
                "event": event.event,
                "timestamp": now_ms(),
                "source": event.plugin,
                "data": event.data,
            })
            .to_string();

            let d = Delivery {
                endpoint_id: ep.id.clone(),
                event: event.event.clone(),
                body,
                delivery_id,
                attempt: 1,
            };
            if self.queue_tx.try_send(d).is_err() {
                self.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    async fn deliver(self: &Arc<Self>, d: Delivery) {
        // Circuit breaker.
        {
            let breaker = self.breaker.entry(d.endpoint_id.clone()).or_default();
            if breaker.open_until_ms > now_ms() {
                self.stat(&d.endpoint_id, |s| {
                    s.skipped_breaker.fetch_add(1, Ordering::Relaxed);
                });
                self.audit(&d, "skipped_breaker", None).await;
                return;
            }
        }

        let endpoint = {
            let endpoints = self.endpoints.read().await;
            endpoints.iter().find(|e| e.id == d.endpoint_id).cloned()
        };
        let Some(endpoint) = endpoint else { return };

        if let Err(e) = ssrf_check(&endpoint.url, self.allow_private).await {
            warn!("Webhook {} blocked (SSRF guard): {e}", endpoint.id);
            self.audit(&d, "blocked_ssrf", None).await;
            self.stat(&d.endpoint_id, |s| {
                s.failed.fetch_add(1, Ordering::Relaxed);
            });
            return;
        }

        let signature = sign(&endpoint.secret, &d.body);
        let result = self
            .http
            .post(&endpoint.url)
            .header("content-type", "application/json")
            .header("x-adatp-event", &d.event)
            .header("x-adatp-delivery", &d.delivery_id)
            .header("x-adatp-signature", format!("sha256={signature}"))
            .body(d.body.clone())
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                self.breaker
                    .entry(d.endpoint_id.clone())
                    .or_default()
                    .consecutive_failures = 0;
                self.stat(&d.endpoint_id, |s| {
                    s.delivered.fetch_add(1, Ordering::Relaxed);
                });
                self.audit(&d, "delivered", Some(resp.status().as_u16())).await;
            }
            outcome => {
                let status = outcome.as_ref().ok().map(|r| r.status().as_u16());
                {
                    let mut breaker = self.breaker.entry(d.endpoint_id.clone()).or_default();
                    breaker.consecutive_failures += 1;
                    if breaker.consecutive_failures >= BREAKER_THRESHOLD {
                        breaker.open_until_ms = now_ms() + BREAKER_OPEN_MS;
                        warn!(
                            "Webhook {} circuit breaker OPEN for {}s",
                            d.endpoint_id,
                            BREAKER_OPEN_MS / 1000
                        );
                    }
                }
                if d.attempt < MAX_ATTEMPTS {
                    self.audit(&d, "retrying", status).await;
                    let mgr = self.clone();
                    let retry = Delivery { attempt: d.attempt + 1, ..d };
                    // Exponential backoff: 1s, 5s, 25s, 125s (capped at 5 min).
                    let delay = std::cmp::min(300, 5u64.pow(retry.attempt.saturating_sub(2)).max(1));
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        if mgr.queue_tx.try_send(retry).is_err() {
                            mgr.dropped_events.fetch_add(1, Ordering::Relaxed);
                        }
                    });
                } else {
                    self.stat(&d.endpoint_id, |s| {
                        s.failed.fetch_add(1, Ordering::Relaxed);
                    });
                    self.audit(&d, "failed", status).await;
                }
            }
        }
    }

    fn stat(&self, endpoint_id: &str, f: impl FnOnce(&Arc<EndpointStats>)) {
        let entry = self.stats.entry(endpoint_id.to_string()).or_default();
        f(entry.value());
    }

    async fn audit(&self, d: &Delivery, outcome: &str, status: Option<u16>) {
        let mut audit = self.audit.lock().await;
        if audit.len() >= AUDIT_CAPACITY {
            audit.pop_front();
        }
        audit.push_back(AuditEntry {
            at_ms: now_ms(),
            endpoint_id: d.endpoint_id.clone(),
            event: d.event.clone(),
            outcome: outcome.to_string(),
            status,
            attempt: d.attempt,
        });
    }

    // ------------------------------------------------------------------
    // Admin plane
    // ------------------------------------------------------------------

    pub async fn views(&self) -> Vec<WebhookView> {
        let endpoints = self.endpoints.read().await;
        endpoints
            .iter()
            .map(|e| {
                let stats = self.stats.entry(e.id.clone()).or_default().clone();
                let breaker_open = self
                    .breaker
                    .get(&e.id)
                    .map(|b| b.open_until_ms > now_ms())
                    .unwrap_or(false);
                WebhookView {
                    id: e.id.clone(),
                    url: e.url.clone(),
                    events: serde_json::from_str(&e.events).unwrap_or_default(),
                    is_active: e.is_active,
                    description: e.description.clone(),
                    created_at: e.created_at.clone(),
                    delivered: stats.delivered.load(Ordering::Relaxed),
                    failed: stats.failed.load(Ordering::Relaxed),
                    skipped_breaker: stats.skipped_breaker.load(Ordering::Relaxed),
                    breaker_open,
                }
            })
            .collect()
    }

    pub async fn audit_log(&self) -> Vec<AuditEntry> {
        self.audit.lock().await.iter().cloned().collect()
    }

    /// Validates a URL for endpoint creation (same SSRF rules as delivery).
    pub async fn validate_url(&self, url: &str) -> Result<(), String> {
        ssrf_check(url, self.allow_private).await
    }

    /// Sends a signed test event to one endpoint (admin "test" button).
    pub async fn send_test(&self, endpoint_id: &str) -> Result<(), String> {
        let endpoint = {
            let endpoints = self.endpoints.read().await;
            endpoints.iter().find(|e| e.id == endpoint_id).cloned()
        }
        .ok_or("unknown webhook id")?;

        let d = Delivery {
            endpoint_id: endpoint.id.clone(),
            event: "webhook.test".into(),
            body: json!({
                "id": Uuid::new_v4().to_string(),
                "event": "webhook.test",
                "timestamp": now_ms(),
                "source": "server",
                "data": { "message": "AdaTP webhook test delivery" },
            })
            .to_string(),
            delivery_id: Uuid::new_v4().to_string(),
            attempt: MAX_ATTEMPTS, // no retries for tests
        };
        self.queue_tx.try_send(d).map_err(|_| "queue full".to_string())
    }
}

