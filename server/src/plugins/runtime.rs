use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use log::{info, warn};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::manifest::Manifest;
use super::PluginEvent;

/// A running plugin child process speaking NDJSON on stdin/stdout.
///
/// Fault isolation: the plugin is a separate OS process. Malformed lines are
/// logged and skipped; process death fails all pending calls and is reported
/// to the manager (which handles restart/backoff).
pub struct PluginProcess {
    #[allow(dead_code)] // diagnostic identity, useful in debugger sessions
    pub name: String,
    stdin_tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    child: Mutex<Option<Child>>,
    next_id: AtomicU64,
    /// Resolved when the process exits (watched by the manager).
    pub exited: Mutex<Option<oneshot::Receiver<()>>>,
}

impl PluginProcess {
    /// Spawns the plugin and wires the NDJSON reader/writer tasks.
    pub async fn spawn(
        dir: &PathBuf,
        manifest: &Manifest,
        events: tokio::sync::broadcast::Sender<PluginEvent>,
        broadcasts: mpsc::Sender<(String, String, String)>, // (plugin, room, text)
    ) -> Result<Arc<Self>, String> {
        let mut cmd = Command::new(&manifest.entry[0]);
        cmd.args(&manifest.entry[1..])
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {:?} failed: {e}", manifest.entry))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(256);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task
        let plugin_name = manifest.name.clone();
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = stdin_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Stderr → server log
        let name_for_err = manifest.name.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[plugin:{name_for_err}] {line}");
            }
        });

        // Reader task: dispatch plugin → server messages.
        let pending_for_reader = pending.clone();
        let name_for_reader = manifest.name.clone();
        let can_emit = manifest.has_permission("emit:events");
        let can_broadcast = manifest.has_permission("rooms:broadcast");
        let (exit_tx, exit_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("[plugin:{name_for_reader}] malformed line ignored: {e}");
                        continue;
                    }
                };
                match msg.get("op").and_then(|o| o.as_str()) {
                    Some("tool_result") | Some("tool_error") | Some("hook_result") | Some("ready") => {
                        let id = msg.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                        if let Some(tx) = pending_for_reader.lock().await.remove(&id) {
                            let _ = tx.send(msg);
                        }
                    }
                    Some("emit_event") => {
                        if can_emit {
                            let event = msg.get("event").and_then(|e| e.as_str()).unwrap_or("unnamed");
                            let _ = events.send(PluginEvent {
                                plugin: name_for_reader.clone(),
                                event: format!("plugin.{name_for_reader}.{event}"),
                                data: msg.get("data").cloned().unwrap_or(Value::Null),
                            });
                        } else {
                            warn!("[plugin:{name_for_reader}] emit_event denied (missing emit:events permission)");
                        }
                    }
                    Some("broadcast") => {
                        if can_broadcast {
                            let room = msg.get("room").and_then(|r| r.as_str()).unwrap_or("global").to_string();
                            let text = msg.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                            let _ = broadcasts.send((name_for_reader.clone(), room, text)).await;
                        } else {
                            warn!("[plugin:{name_for_reader}] broadcast denied (missing rooms:broadcast permission)");
                        }
                    }
                    Some("log") => {
                        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("info");
                        let text = msg.get("message").and_then(|m| m.as_str()).unwrap_or("");
                        match level {
                            "error" => log::error!("[plugin:{name_for_reader}] {text}"),
                            "warn" => log::warn!("[plugin:{name_for_reader}] {text}"),
                            _ => log::info!("[plugin:{name_for_reader}] {text}"),
                        }
                    }
                    _ => warn!("[plugin:{name_for_reader}] unknown op ignored: {line}"),
                }
            }
            // stdout closed → process is gone; fail all pending calls.
            let mut p = pending_for_reader.lock().await;
            for (_, tx) in p.drain() {
                let _ = tx.send(json!({"op":"tool_error","code":"tool_failed","message":"plugin exited"}));
            }
            let _ = exit_tx.send(());
            info!("[plugin:{name_for_reader}] process ended");
        });

        let proc = Arc::new(Self {
            name: manifest.name.clone(),
            stdin_tx,
            pending,
            child: Mutex::new(Some(child)),
            next_id: AtomicU64::new(1),
            exited: Mutex::new(Some(exit_rx)),
        });

        // Init line (plugin may ignore it).
        let _ = proc
            .send_line(json!({"op":"init","plugin":plugin_name,"server_version":"1.0.0"}))
            .await;

        Ok(proc)
    }

    async fn send_line(&self, v: Value) -> Result<(), String> {
        self.stdin_tx
            .send(v.to_string())
            .await
            .map_err(|_| "plugin stdin closed".to_string())
    }

    /// Sends a request that expects a correlated reply; resolves with the raw
    /// reply JSON or an error after `timeout_ms`.
    pub async fn request(&self, mut msg: Value, timeout_ms: u64) -> Result<Value, String> {
        let id = format!("srv-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        msg["id"] = Value::String(id.clone());

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        if let Err(e) = self.send_line(msg).await {
            self.pending.lock().await.remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err("plugin dropped the request".into()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err("timeout".into())
            }
        }
    }

    /// Fire-and-forget notification.
    pub async fn notify(&self, msg: Value) {
        let _ = self.send_line(msg).await;
    }

    /// Asks the plugin to exit; kills it after `grace_ms`.
    pub async fn shutdown(&self, grace_ms: u64) {
        let _ = self.send_line(json!({"op":"shutdown"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
    }
}
