pub mod manifest;
pub mod runtime;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use log::{info, warn};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::hub::{Hub, RouteMsg};
use adatp_core::MessageType;

use manifest::{validate_schema_subset, Manifest};
use runtime::PluginProcess;

const MAX_TOOL_ARGS_BYTES: usize = 64 * 1024;
const MAX_RESTARTS: u32 = 5;

/// A custom event emitted by a plugin (fanned out to webhooks).
#[derive(Debug, Clone, Serialize)]
pub struct PluginEvent {
    pub plugin: String,
    pub event: String,
    pub data: Value,
}

/// Who is calling a tool — passed to plugins for authorization decisions.
#[derive(Debug, Clone, Serialize)]
pub struct CallerCtx {
    pub username: String,
    pub role: String,
    pub session: String,
    pub room: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    Running,
    Disabled,
    Errored,
}

#[derive(Default)]
pub struct PluginMetrics {
    pub calls: AtomicU64,
    pub errors: AtomicU64,
    pub latency_ms_total: AtomicU64,
    pub restarts: AtomicU64,
    pub hook_denies: AtomicU64,
}

pub struct PluginEntry {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub state: RwLock<PluginState>,
    pub process: RwLock<Option<Arc<PluginProcess>>>,
    pub metrics: PluginMetrics,
    pub last_error: RwLock<Option<String>>,
}

/// A tool-call failure in the wire contract's error vocabulary.
#[derive(Debug, Clone, Serialize)]
pub struct ToolErr {
    pub code: String,
    pub message: String,
}

impl ToolErr {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// Serializable plugin view for the admin plane.
#[derive(Serialize)]
pub struct PluginView {
    pub name: String,
    pub version: String,
    pub description: String,
    pub state: PluginState,
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
    pub permissions: Vec<String>,
    pub calls: u64,
    pub errors: u64,
    pub restarts: u64,
    pub avg_latency_ms: u64,
    pub last_error: Option<String>,
}

pub struct PluginManager {
    plugins_dir: PathBuf,
    plugins: DashMap<String, Arc<PluginEntry>>,
    /// tool name → plugin name
    tool_index: DashMap<String, String>,
    /// hook name → plugin names
    hook_index: DashMap<String, Vec<String>>,
    events_tx: broadcast::Sender<PluginEvent>,
    broadcasts_tx: mpsc::Sender<(String, String, String)>,
    /// (plugin, tool) → (window minute, count) fixed-window rate limiting
    rate: DashMap<(String, String), (u64, u32)>,
}

impl PluginManager {
    pub fn new(plugins_dir: &str, hub: Arc<Hub>) -> Arc<Self> {
        let (events_tx, _) = broadcast::channel(512);
        let (broadcasts_tx, mut broadcasts_rx) = mpsc::channel::<(String, String, String)>(256);

        // Pump plugin-initiated room broadcasts into the hub. The sender
        // identity is the nil UUID (= "the server").
        let hub_for_pump = hub;
        tokio::spawn(async move {
            while let Some((plugin, room, text)) = broadcasts_rx.recv().await {
                info!("[plugin:{plugin}] broadcast to '{room}'");
                hub_for_pump.broadcast(
                    &room,
                    RouteMsg {
                        sender: Uuid::nil(),
                        msg_type: MessageType::TextMessage,
                        payload: Bytes::from(text.into_bytes()),
                    },
                );
            }
        });

        Arc::new(Self {
            plugins_dir: PathBuf::from(plugins_dir),
            plugins: DashMap::new(),
            tool_index: DashMap::new(),
            hook_index: DashMap::new(),
            events_tx,
            broadcasts_tx,
            rate: DashMap::new(),
        })
    }

    /// Subscribe to plugin-emitted custom events (webhook fan-out).
    pub fn subscribe_events(&self) -> broadcast::Receiver<PluginEvent> {
        self.events_tx.subscribe()
    }

    /// Emit a server-side event through the same bus (used by the webhook
    /// module for built-in events).
    pub fn emit_server_event(&self, event: &str, data: Value) {
        let _ = self.events_tx.send(PluginEvent {
            plugin: "server".into(),
            event: event.to_string(),
            data,
        });
    }

    // ------------------------------------------------------------------
    // Loading / lifecycle
    // ------------------------------------------------------------------

    /// Scans PLUGINS_DIR and starts every valid plugin.
    pub async fn load_all(self: &Arc<Self>) {
        let dir = self.plugins_dir.clone();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => {
                info!("No plugins directory at {:?} — plugin platform idle", dir);
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("plugin.json").exists() {
                if let Err(e) = self.load_plugin(&path).await {
                    warn!("Plugin at {:?} not loaded: {e}", path);
                }
            }
        }
        info!("Plugin platform: {} plugin(s) loaded", self.plugins.len());
    }

    async fn load_plugin(self: &Arc<Self>, dir: &PathBuf) -> Result<String, String> {
        let raw = std::fs::read_to_string(dir.join("plugin.json"))
            .map_err(|e| format!("manifest unreadable: {e}"))?;
        let manifest: Manifest =
            serde_json::from_str(&raw).map_err(|e| format!("manifest invalid JSON: {e}"))?;
        manifest.validate()?;

        let name = manifest.name.clone();
        if self.plugins.contains_key(&name) {
            return Err(format!("duplicate plugin name '{name}'"));
        }

        // Tool name collisions across plugins are rejected.
        for t in &manifest.tools {
            if self.tool_index.contains_key(&t.name) {
                return Err(format!(
                    "tool '{}' already provided by another plugin",
                    t.name
                ));
            }
        }

        let entry = Arc::new(PluginEntry {
            manifest: manifest.clone(),
            dir: dir.clone(),
            state: RwLock::new(PluginState::Disabled),
            process: RwLock::new(None),
            metrics: PluginMetrics::default(),
            last_error: RwLock::new(None),
        });
        self.plugins.insert(name.clone(), entry.clone());
        for t in &manifest.tools {
            self.tool_index.insert(t.name.clone(), name.clone());
        }
        for h in &manifest.hooks {
            self.hook_index
                .entry(h.clone())
                .or_default()
                .push(name.clone());
        }

        self.start(&name).await?;
        Ok(name)
    }

    /// Starts (or restarts) a plugin process and arms the crash watcher.
    /// Returns a boxed future because the crash watcher recursively calls
    /// `start` again (async recursion needs type erasure).
    fn start<'a>(
        self: &'a Arc<Self>,
        name: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<(), String>> {
        Box::pin(self.start_inner(name))
    }

    async fn start_inner(self: &Arc<Self>, name: &str) -> Result<(), String> {
        let entry = self.plugins.get(name).ok_or("unknown plugin")?.clone();
        let proc = PluginProcess::spawn(
            &entry.dir,
            &entry.manifest,
            self.events_tx.clone(),
            self.broadcasts_tx.clone(),
        )
        .await?;

        *entry.process.write().await = Some(proc.clone());
        *entry.state.write().await = PluginState::Running;
        *entry.last_error.write().await = None;
        info!(
            "Plugin '{name}' running ({} tool(s))",
            entry.manifest.tools.len()
        );

        // Crash watcher with restart backoff.
        let mgr = self.clone();
        let plugin_name = name.to_string();
        let exit_rx = proc.exited.lock().await.take();
        if let Some(exit_rx) = exit_rx {
            tokio::spawn(async move {
                let _ = exit_rx.await;
                let entry = match mgr.plugins.get(&plugin_name) {
                    Some(e) => e.clone(),
                    None => return,
                };
                // Deliberate disable/shutdown → no restart.
                if *entry.state.read().await != PluginState::Running {
                    return;
                }
                let restarts = entry.metrics.restarts.fetch_add(1, Ordering::Relaxed) + 1;
                if restarts > MAX_RESTARTS as u64 {
                    *entry.state.write().await = PluginState::Errored;
                    *entry.last_error.write().await =
                        Some(format!("crashed {restarts} times; giving up"));
                    warn!("Plugin '{plugin_name}' errored permanently after {restarts} crashes");
                    return;
                }
                let backoff = std::cmp::min(30_000, 1000u64 << (restarts - 1));
                warn!("Plugin '{plugin_name}' exited; restart #{restarts} in {backoff}ms");
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                if let Err(e) = mgr.start(&plugin_name).await {
                    *entry.state.write().await = PluginState::Errored;
                    *entry.last_error.write().await = Some(e.clone());
                    warn!("Plugin '{plugin_name}' restart failed: {e}");
                }
            });
        }
        Ok(())
    }

    pub async fn disable(&self, name: &str) -> Result<(), String> {
        let entry = self.plugins.get(name).ok_or("unknown plugin")?.clone();
        *entry.state.write().await = PluginState::Disabled;
        if let Some(proc) = entry.process.write().await.take() {
            proc.shutdown(2000).await;
        }
        info!("Plugin '{name}' disabled");
        Ok(())
    }

    pub async fn enable(self: &Arc<Self>, name: &str) -> Result<(), String> {
        let entry = self.plugins.get(name).ok_or("unknown plugin")?.clone();
        if *entry.state.read().await == PluginState::Running {
            return Ok(());
        }
        entry.metrics.restarts.store(0, Ordering::Relaxed);
        self.start(name).await
    }

    /// Re-reads the manifest from disk and restarts the process.
    pub async fn reload(self: &Arc<Self>, name: &str) -> Result<(), String> {
        let entry = self.plugins.get(name).ok_or("unknown plugin")?.clone();
        let dir = entry.dir.clone();
        self.disable(name).await?;

        // Drop old registry state entirely, then load fresh.
        self.plugins.remove(name);
        self.tool_index.retain(|_, v| v != name);
        for mut hooks in self.hook_index.iter_mut() {
            hooks.retain(|p| p != name);
        }
        self.load_plugin(&dir).await.map(|_| ())
    }

    pub fn list(&self) -> Vec<PluginView> {
        self.plugins
            .iter()
            .map(|e| {
                let calls = e.metrics.calls.load(Ordering::Relaxed);
                let lat = e.metrics.latency_ms_total.load(Ordering::Relaxed);
                PluginView {
                    name: e.manifest.name.clone(),
                    version: e.manifest.version.clone(),
                    description: e.manifest.description.clone(),
                    state: e
                        .state
                        .try_read()
                        .map(|s| s.clone())
                        .unwrap_or(PluginState::Errored),
                    tools: e.manifest.tools.iter().map(|t| t.name.clone()).collect(),
                    hooks: e.manifest.hooks.clone(),
                    permissions: e.manifest.permissions.clone(),
                    calls,
                    errors: e.metrics.errors.load(Ordering::Relaxed),
                    restarts: e.metrics.restarts.load(Ordering::Relaxed),
                    avg_latency_ms: if calls > 0 { lat / calls } else { 0 },
                    last_error: e.last_error.try_read().ok().and_then(|g| g.clone()),
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Tool calls
    // ------------------------------------------------------------------

    pub fn list_tools(&self) -> Value {
        let mut tools = vec![json!({
            "name": "system.list_tools",
            "description": "Lists every tool available on this server.",
            "schema": { "type": "object" },
            "plugin": "server",
        })];
        for entry in self.plugins.iter() {
            for t in &entry.manifest.tools {
                tools.push(json!({
                    "name": t.name,
                    "description": t.description,
                    "schema": t.schema,
                    "plugin": entry.manifest.name,
                }));
            }
        }
        json!({ "tools": tools })
    }

    /// Executes a tool call end to end (rate limit, schema validation,
    /// hooks, dispatch, metrics). Returns the tool result or a ToolErr.
    pub async fn call_tool(
        &self,
        caller: &CallerCtx,
        tool: &str,
        args: Value,
    ) -> Result<Value, ToolErr> {
        if tool == "system.list_tools" {
            return Ok(self.list_tools());
        }

        let plugin_name = self
            .tool_index
            .get(tool)
            .map(|e| e.value().clone())
            .ok_or_else(|| ToolErr::new("tool_not_found", format!("no tool named '{tool}'")))?;
        let entry = self
            .plugins
            .get(&plugin_name)
            .ok_or_else(|| ToolErr::new("tool_not_found", "plugin unloaded"))?
            .clone();

        if *entry.state.read().await != PluginState::Running {
            return Err(ToolErr::new("tool_failed", "plugin is not running"));
        }
        let spec = entry
            .manifest
            .tools
            .iter()
            .find(|t| t.name == tool)
            .ok_or_else(|| ToolErr::new("tool_not_found", "tool spec missing"))?
            .clone();

        // Rate limit (fixed one-minute window per tool).
        if spec.rate_limit_per_min > 0 {
            let minute = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() / 60)
                .unwrap_or(0);
            let mut slot = self
                .rate
                .entry((plugin_name.clone(), tool.to_string()))
                .or_insert((minute, 0));
            if slot.0 != minute {
                *slot = (minute, 0);
            }
            if slot.1 >= spec.rate_limit_per_min {
                return Err(ToolErr::new(
                    "tool_rate_limited",
                    "rate limit exceeded, retry later",
                ));
            }
            slot.1 += 1;
        }

        // Argument validation (size + schema subset).
        let args_size = serde_json::to_vec(&args)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        if args_size > MAX_TOOL_ARGS_BYTES {
            return Err(ToolErr::new("tool_invalid_args", "args exceed 64 KiB"));
        }
        if let Err(e) = validate_schema_subset(&spec.schema, &args, "args") {
            return Err(ToolErr::new("tool_invalid_args", e));
        }

        // tool_before veto hook.
        let before_event = json!({
            "tool": tool, "plugin": plugin_name, "args": args, "caller": caller,
        });
        if !self.veto_hook("tool_before", &before_event).await {
            entry.metrics.hook_denies.fetch_add(1, Ordering::Relaxed);
            return Err(ToolErr::new("tool_forbidden", "denied by policy hook"));
        }

        let proc = entry
            .process
            .read()
            .await
            .clone()
            .ok_or_else(|| ToolErr::new("tool_failed", "plugin process unavailable"))?;

        entry.metrics.calls.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let reply = proc
            .request(
                json!({ "op": "tool_call", "tool": tool, "args": args, "caller": caller }),
                spec.timeout_ms,
            )
            .await;
        let elapsed = started.elapsed().as_millis() as u64;
        entry
            .metrics
            .latency_ms_total
            .fetch_add(elapsed, Ordering::Relaxed);

        let outcome = match reply {
            Ok(v) => match v.get("op").and_then(|o| o.as_str()) {
                Some("tool_result") => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                Some("tool_error") => Err(ToolErr::new(
                    v.get("code")
                        .and_then(|c| c.as_str())
                        .unwrap_or("tool_failed"),
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("tool error"),
                )),
                _ => Err(ToolErr::new("tool_failed", "malformed plugin reply")),
            },
            Err(e) if e == "timeout" => Err(ToolErr::new(
                "tool_timeout",
                format!("no reply within {}ms", spec.timeout_ms),
            )),
            Err(e) => Err(ToolErr::new("tool_failed", e)),
        };

        if outcome.is_err() {
            entry.metrics.errors.fetch_add(1, Ordering::Relaxed);
        }

        // tool_after notification hook + event for webhooks.
        let after_event = json!({
            "tool": tool, "plugin": plugin_name, "caller": caller,
            "ok": outcome.is_ok(), "latency_ms": elapsed,
        });
        self.notify_hook("tool_after", &after_event).await;
        self.emit_server_event("tool.called", after_event);

        outcome
    }

    // ------------------------------------------------------------------
    // Hooks
    // ------------------------------------------------------------------

    pub fn has_hook(&self, hook: &str) -> bool {
        self.hook_index
            .get(hook)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Dispatches a veto-able hook. Returns true when EVERY registered
    /// plugin allows (or is unavailable with a fail-open policy).
    pub async fn veto_hook(&self, hook: &str, event: &Value) -> bool {
        let plugin_names = match self.hook_index.get(hook) {
            Some(v) => v.clone(),
            None => return true,
        };
        for name in plugin_names {
            let entry = match self.plugins.get(&name) {
                Some(e) => e.clone(),
                None => continue,
            };
            if *entry.state.read().await != PluginState::Running {
                if entry.manifest.hook_failure_policy == "deny" {
                    return false;
                }
                continue;
            }
            let proc = match entry.process.read().await.clone() {
                Some(p) => p,
                None => continue,
            };
            let reply = proc
                .request(
                    json!({ "op": "hook", "hook": hook, "event": event }),
                    entry.manifest.hook_timeout_ms,
                )
                .await;
            match reply {
                Ok(v) => {
                    if !v.get("allow").and_then(|a| a.as_bool()).unwrap_or(true) {
                        entry.metrics.hook_denies.fetch_add(1, Ordering::Relaxed);
                        return false;
                    }
                }
                Err(_) => {
                    if entry.manifest.hook_failure_policy == "deny" {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Fire-and-forget hook notification.
    pub async fn notify_hook(&self, hook: &str, event: &Value) {
        let plugin_names = match self.hook_index.get(hook) {
            Some(v) => v.clone(),
            None => return,
        };
        for name in plugin_names {
            if let Some(entry) = self.plugins.get(&name) {
                if let Some(proc) = entry.process.read().await.clone() {
                    proc.notify(
                        json!({ "op": "hook", "hook": hook, "event": event, "notify": true }),
                    )
                    .await;
                }
            }
        }
    }

    /// Notifies every plugin of server shutdown and stops the processes.
    pub async fn shutdown_all(&self) {
        for entry in self.plugins.iter() {
            *entry.state.write().await = PluginState::Disabled;
            if let Some(proc) = entry.process.write().await.take() {
                proc.notify(
                    json!({ "op": "hook", "hook": "shutdown", "event": {}, "notify": true }),
                )
                .await;
                proc.shutdown(1000).await;
            }
        }
    }
}
