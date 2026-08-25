use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Permissions a plugin may request. Everything is default-deny: an
/// operation is only allowed if its permission is listed in the manifest.
pub const KNOWN_PERMISSIONS: &[&str] = &[
    "tools",           // expose callable tools
    "hooks:auth",      // veto logins
    "hooks:text",      // observe/veto text messages
    "hooks:file",      // observe/veto file transfers
    "hooks:presence",  // observe presence updates
    "hooks:rooms",     // observe join/leave
    "hooks:tools",     // observe/veto tool calls (before/after)
    "emit:events",     // emit custom events (webhook fan-out)
    "rooms:broadcast", // send text messages into rooms
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON-Schema subset: {type, properties, required, enum, items}.
    #[serde(default = "default_schema")]
    pub schema: Value,
    #[serde(default = "default_tool_timeout")]
    pub timeout_ms: u64,
    /// Calls allowed per minute (per tool, across all callers). 0 = unlimited.
    #[serde(default = "default_rate_limit")]
    pub rate_limit_per_min: u32,
}

fn default_schema() -> Value {
    serde_json::json!({ "type": "object" })
}
fn default_tool_timeout() -> u64 {
    10_000
}
fn default_rate_limit() -> u32 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// argv, resolved relative to the plugin directory (e.g. ["node","index.js"]).
    pub entry: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    /// Hook names: auth, text, file, presence, join, leave, tool_before, tool_after, shutdown
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default = "default_hook_timeout")]
    pub hook_timeout_ms: u64,
    /// "allow" (fail-open) or "deny" (fail-closed) when a veto hook times out
    /// or the plugin is unavailable.
    #[serde(default = "default_hook_policy")]
    pub hook_failure_policy: String,
}

fn default_hook_timeout() -> u64 {
    500
}
fn default_hook_policy() -> String {
    "allow".to_string()
}

const VALID_HOOKS: &[&str] = &[
    "auth",
    "text",
    "file",
    "presence",
    "join",
    "leave",
    "tool_before",
    "tool_after",
    "shutdown",
];

/// Hook → permission that must be present to register it.
pub fn hook_permission(hook: &str) -> &'static str {
    match hook {
        "auth" => "hooks:auth",
        "text" => "hooks:text",
        "file" => "hooks:file",
        "presence" => "hooks:presence",
        "join" | "leave" => "hooks:rooms",
        "tool_before" | "tool_after" => "hooks:tools",
        _ => "hooks:rooms", // shutdown is always allowed; mapped harmlessly
    }
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        let name_ok = !self.name.is_empty()
            && self.name.len() <= 32
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !name_ok {
            return Err("name must be 1-32 chars of [a-z0-9_-]".into());
        }
        if self.version.is_empty() || self.version.len() > 32 {
            return Err("version is required".into());
        }
        if self.entry.is_empty() || self.entry[0].is_empty() {
            return Err("entry must be a non-empty argv array".into());
        }
        for p in &self.permissions {
            if !KNOWN_PERMISSIONS.contains(&p.as_str()) {
                return Err(format!("unknown permission '{p}'"));
            }
        }
        if !self.tools.is_empty() && !self.permissions.iter().any(|p| p == "tools") {
            return Err("declaring tools requires the 'tools' permission".into());
        }
        let mut seen = std::collections::HashSet::new();
        for t in &self.tools {
            if t.name.is_empty() || t.name.len() > 64 {
                return Err("tool names must be 1-64 chars".into());
            }
            if t.name.starts_with("system.") {
                return Err(format!(
                    "tool '{}' uses the reserved 'system.' prefix",
                    t.name
                ));
            }
            if !seen.insert(t.name.clone()) {
                return Err(format!("duplicate tool '{}'", t.name));
            }
            if !t.schema.is_object() {
                return Err(format!("tool '{}' schema must be an object", t.name));
            }
        }
        for h in &self.hooks {
            if !VALID_HOOKS.contains(&h.as_str()) {
                return Err(format!("unknown hook '{h}'"));
            }
            if h != "shutdown" && !self.permissions.iter().any(|p| p == hook_permission(h)) {
                return Err(format!(
                    "hook '{h}' requires permission '{}'",
                    hook_permission(h)
                ));
            }
        }
        match self.hook_failure_policy.as_str() {
            "allow" | "deny" => {}
            other => {
                return Err(format!(
                    "hook_failure_policy must be allow|deny, got '{other}'"
                ))
            }
        }
        Ok(())
    }

    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

/// Validates a JSON value against the supported JSON-Schema subset:
/// `type`, `properties`, `required`, `enum`, `items`. Unknown keywords are
/// ignored (documented in the plugin guide).
pub fn validate_schema_subset(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if let Some(allowed) = schema.get("enum").and_then(|e| e.as_array()) {
        if !allowed.contains(value) {
            return Err(format!("{path}: value not in enum"));
        }
    }

    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        let ok = match ty {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => true,
        };
        if !ok {
            return Err(format!("{path}: expected {ty}"));
        }
    }

    if value.is_object() {
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for key in required.iter().filter_map(|k| k.as_str()) {
                if value.get(key).is_none() {
                    return Err(format!("{path}: missing required field '{key}'"));
                }
            }
        }
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            for (key, sub) in props {
                if let Some(v) = value.get(key) {
                    validate_schema_subset(sub, v, &format!("{path}.{key}"))?;
                }
            }
        }
    }

    if let (Some(items), Some(arr)) = (schema.get("items"), value.as_array()) {
        for (i, v) in arr.iter().enumerate() {
            validate_schema_subset(items, v, &format!("{path}[{i}]"))?;
        }
    }

    Ok(())
}
