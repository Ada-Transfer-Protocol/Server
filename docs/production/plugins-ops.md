# Plugins — Operations

Developer-facing reference (manifest, NDJSON protocol, hooks):
[`../platform/PLUGIN_DEVELOPMENT.md`](../platform/PLUGIN_DEVELOPMENT.md).
This page covers running plugins in production.

## Deployment layout

```
$PLUGINS_DIR/
├── moderation/
│   ├── plugin.json      # manifest (validated at load)
│   ├── index.mjs        # entry (any language)
│   └── words.json       # plugin-private config
└── billing-bridge/
    ├── plugin.json
    └── main.py
```

- Scanned **once at boot**; a directory without `plugin.json` is ignored;
  an invalid manifest is logged and skipped (the server still starts).
- Adding a new plugin = deploy the directory + restart, or deploy + admin
  `reload` of an existing name. Brand-new names require a restart (load
  happens at boot); plan plugin rollout with your normal deploy window.
- The plugin process runs with **cwd = its own directory**, as the same OS
  user as the server. Keep per-plugin state inside the plugin dir.

## Runtime prerequisites

The server spawns whatever `entry` says (`["node","index.mjs"]`,
`["python3","main.py"]`, a compiled binary…). The interpreter must exist
**in the server's environment**:

- Native/systemd installs: install node/python system-wide as needed.
- Docker: the slim runtime image ships **no** node/python — build a derived
  image adding your plugin runtimes, or compile plugins to static binaries
  ([install-docker.md](./install-docker.md)).

## Health & diagnosis

`GET /admin/v1/plugins` (or Silo → PLUGINS) per plugin:

| Field | Watch for |
| :-- | :-- |
| `state` | `running` / `disabled` / `errored` (gave up after 5 crashes) |
| `restarts` | climbing = crash loop (backoff 1 s → 2 s → 4 s … 30 s) |
| `last_error` | spawn failure or give-up reason |
| `calls` / `errors` | error ratio per tool traffic |
| `avg_latency_ms` | creeping toward the tool's `timeout_ms` = imminent `tool_timeout` |
| `hook_denies` | how often its policy hooks blocked something |

Plugin stack traces: the child's stderr is folded into the server log as
`[plugin:<name>] …` lines — `journalctl -u adatp-server | grep 'plugin:moderation'`.

### Crash-loop runbook

1. `GET /admin/v1/plugins` → confirm `restarts` climbing / `errored`.
2. `POST /admin/v1/plugins/<name>/disable` — stop the churn.
3. Read `[plugin:<name>]` stderr lines; reproduce locally by piping NDJSON
   to the entry command by hand.
4. Fix, redeploy files, `POST …/reload`.
5. Verify `state=running` and exercise one tool call.

## Failure semantics to plan around

- A dead plugin's **tools** return `tool_failed` to callers immediately.
- Its **veto hooks** follow the manifest's `hook_failure_policy`:
  `allow` (default) = traffic flows unmoderated while it's down;
  `deny` = the hooked action is blocked while it's down. Choose per plugin:
  moderation of a public room probably wants `deny`; a metrics observer
  wants `allow`. This is a **product decision — audit it per plugin** at
  review time.

## Resource isolation

Process isolation protects the server from crashes, **not** from a plugin
eating CPU/RAM. v1 sets no rlimits on children — impose them at the OS
layer:

- systemd: put the server in a slice with `MemoryMax=`/`CPUQuota=`
  (children inherit the cgroup), or run heavyweight plugins as external
  services bridged by a thin plugin.
- Docker/K8s: the container's limits bound server + plugins together;
  size limits with plugins in mind.
- Malicious-input robustness: hook/tool payloads carry user content —
  plugins must treat them as untrusted
  ([`AI_AGENT_DEVELOPMENT.md`](../developer/AI_AGENT_DEVELOPMENT.md) safety notes apply).

## Supply chain

A plugin is arbitrary code running next to your server with the server's
filesystem/network rights. Before deploying third-party plugins:

- Read the manifest first: `permissions` is default-deny and reviewable —
  a "logger" requesting `rooms:broadcast` + `hooks:auth` is a finding.
- Pin versions; vendor the code into your own deploy artifact; no
  `curl | node` at runtime.
- Prefer `hook_failure_policy` and rate limits set consciously, not
  defaults.
- Constrain egress at the OS/container level for anything untrusted — the
  webhook SSRF guard does **not** apply to plugin processes.

## Incident default: disable first

`POST /admin/v1/plugins/<name>/disable` is cheap, immediate, and reversible
(`enable`). When in doubt during an incident, disable the suspect plugin
and observe — tools error cleanly, fail-open hooks stop filtering, and the
data plane keeps running.
