# AdaTP Plugin Development Guide

This is the canonical guide for building AdaTP server plugins. Everything
here matches the implementation in `server/server/src/plugins/` exactly;
where behavior and documentation disagree, that is a bug — report it.

Related: [tool packet contract](../spec/09-extensions.md) ·
[webhook guide](WEBHOOK_DEVELOPMENT.md) · [admin API](admin-api.md) ·
[AI agent guide](../developer/AI_AGENT_DEVELOPMENT.md)

---

## 1. What a plugin is

An AdaTP plugin is a **separate OS process** started and supervised by the
server. It talks to the server over **NDJSON on stdin/stdout** — one JSON
object per line in each direction. That gives you:

- **Any language.** If it can read stdin and write stdout, it can be a
  plugin. The bundled examples use Node.js; Python, Go, or a shell script
  work the same way.
- **Fault isolation.** A crashing plugin cannot take the server down. The
  server fails its pending calls, logs the exit, and restarts it with
  backoff.
- **Default-deny permissions.** A plugin can only do what its manifest
  declares.

Plugins provide two things:

1. **Tools** — named, schema-validated functions clients call through the
   `ToolCall`/`ToolResult`/`ToolError` packets (`0x0070–0x0072`).
2. **Hooks** — observation and veto points in the server's data plane
   (logins, text messages, file transfers, presence, room membership, tool
   calls).

Plugins may additionally **emit custom events** (fanned out to
[webhooks](WEBHOOK_DEVELOPMENT.md)) and **broadcast text into rooms**.

## 2. Quickstart — a plugin in 15 minutes

Plugins live in `PLUGINS_DIR` (default `plugins/`, with a fallback to
`server/plugins` so `cargo run` from the workspace root finds the bundled
examples). One directory per plugin, containing `plugin.json` plus your
program.

**Step 1 — manifest** (`plugins/greeter/plugin.json`):

```json
{
    "name": "greeter",
    "version": "1.0.0",
    "description": "Greets people by name.",
    "entry": ["node", "index.mjs"],
    "permissions": ["tools"],
    "tools": [
        {
            "name": "greeter.hello",
            "description": "Returns a greeting.",
            "schema": {
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"]
            }
        }
    ]
}
```

**Step 2 — program** (`plugins/greeter/index.mjs`, complete):

```js
import { createInterface } from 'readline';
const send = (msg) => process.stdout.write(JSON.stringify(msg) + '\n');

createInterface({ input: process.stdin, terminal: false }).on('line', (line) => {
    let msg;
    try { msg = JSON.parse(line); } catch { return; }

    if (msg.op === 'tool_call' && msg.tool === 'greeter.hello') {
        send({ op: 'tool_result', id: msg.id,
               result: { greeting: `Hello, ${msg.args.name}! (asked by ${msg.caller.username})` } });
    } else if (msg.op === 'tool_call') {
        send({ op: 'tool_error', id: msg.id, code: 'tool_not_found', message: msg.tool });
    } else if (msg.op === 'shutdown') {
        process.exit(0);
    }
});
```

**Step 3 — load it.** Restart the server, or without a restart:

```bash
# plugins already known to the server can be hot-reloaded:
curl -X POST -H "x-admin-token: $ADMIN_TOKEN" \
    http://127.0.0.1:3000/admin/v1/plugins/greeter/reload
```

(A brand-new directory is discovered at boot; use a restart the first time.)

**Step 4 — call it** from any SDK:

```js
// Node.js
const tools = await client.listTools();          // includes greeter.hello
const r = await client.callTool('greeter.hello', { name: 'Ada' });
// r.greeting === "Hello, Ada! (asked by user1)"
```

That is the whole third-party path: manifest → script → load → call.

## 3. Manifest reference

```json
{
    "name": "...",              // required
    "version": "...",           // required
    "description": "...",
    "entry": ["...", "..."],    // required
    "permissions": ["..."],
    "tools": [ { ... } ],
    "hooks": ["..."],
    "hook_timeout_ms": 500,
    "hook_failure_policy": "allow"
}
```

| Field | Rules |
| :-- | :-- |
| `name` | 1–32 chars of `[a-z0-9_-]`. Must be unique across loaded plugins. |
| `version` | Required, ≤32 chars. Yours to manage (semver recommended). |
| `description` | Free text, shown in Silo and `system.list_tools`. |
| `entry` | argv array, e.g. `["node", "index.mjs"]` or `["python3", "main.py"]`. The process starts with **cwd = the plugin directory**. |
| `permissions` | Default-deny list — see §3.1. Declaring tools requires `tools`; registering a hook requires its permission. |
| `tools` | Tool specs — see §3.2. |
| `hooks` | Hook names — see §5. Unknown names are rejected at load. |
| `hook_timeout_ms` | How long the server waits for a veto reply. Default **500**. |
| `hook_failure_policy` | `allow` (fail-open, default) or `deny` (fail-closed) when this plugin times out or is down during a veto. |

A manifest that fails validation is logged and the plugin is not loaded.

### 3.1 Permissions (default-deny)

| Permission | Grants |
| :-- | :-- |
| `tools` | Exposing callable tools. |
| `hooks:auth` | Registering the `auth` veto hook. |
| `hooks:text` | Registering the `text` veto hook. |
| `hooks:file` | Registering the `file` veto hook. |
| `hooks:presence` | Registering the `presence` notification hook. |
| `hooks:rooms` | Registering `join` / `leave` notification hooks. |
| `hooks:tools` | Registering `tool_before` (veto) / `tool_after` hooks. |
| `emit:events` | `emit_event` op — custom events for webhook fan-out. |
| `rooms:broadcast` | `broadcast` op — sending text messages into rooms. |

Anything not listed is refused: an `emit_event` from a plugin without
`emit:events` is dropped with a warning, a hook without its permission
fails manifest validation. The `shutdown` hook needs no permission.

### 3.2 Tool specs

```json
{
    "name": "moderation.check",
    "description": "Checks a text against the blocklist.",
    "schema": {
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"]
    },
    "timeout_ms": 2000,
    "rate_limit_per_min": 300
}
```

| Field | Rules |
| :-- | :-- |
| `name` | 1–64 chars, unique across **all** loaded plugins. The `system.` prefix is reserved for built-ins. Convention: `<plugin>.<verb>`. |
| `schema` | **JSON-Schema subset**: `type`, `properties`, `required`, `enum`, `items`. Unknown keywords are ignored (documented limitation — don't rely on `minLength`, `pattern`, etc.; validate inside the plugin). Arguments failing validation are rejected server-side with `tool_invalid_args` before your plugin ever sees them. |
| `timeout_ms` | Server-side deadline per call. Default **10000**. Expiry → `tool_timeout` to the caller. |
| `rate_limit_per_min` | Calls per fixed one-minute window, per tool, across all callers. Default **120**; `0` = unlimited. Excess → `tool_rate_limited`. |

Arguments are additionally capped at **64 KiB** serialized
(`tool_invalid_args` beyond that).

## 4. The wire protocol (server ↔ plugin)

One JSON object per line. Unknown `op`s **must be ignored** (the server
ignores unknown ops from you, log-warning them).

### Server → plugin

| Message | Meaning |
| :-- | :-- |
| `{"op":"init","plugin":"<name>","server_version":"1.0.0"}` | Sent once after spawn. You may ignore it. |
| `{"op":"tool_call","id":"srv-1","tool":"...","args":{...},"caller":{...}}` | Execute a tool. **Must** be answered with `tool_result` or `tool_error` echoing `id`. `caller` = `{username, role, session, room}`. |
| `{"op":"hook","hook":"text","event":{...},"id":"srv-2"}` | **Veto request** — carries `id`, no `notify`. **Must** be answered with `hook_result`. |
| `{"op":"hook","hook":"join","event":{...},"notify":true}` | **Notification** — carries `notify: true`. **Must not** be answered. |
| `{"op":"shutdown"}` | Exit cleanly. You get a grace period (~1–2 s) before a kill. |

### Plugin → server

| Message | Requires | Meaning |
| :-- | :-- | :-- |
| `{"op":"tool_result","id":"srv-1","result":<any JSON>}` | — | Successful tool reply. |
| `{"op":"tool_error","id":"srv-1","code":"...","message":"..."}` | — | Failed tool reply. Codes: `tool_not_found`, `tool_failed`, `tool_invalid_args`, `tool_forbidden`. |
| `{"op":"hook_result","id":"srv-2","allow":true}` | — | Veto answer. `allow: false` blocks the action. |
| `{"op":"emit_event","event":"scored","data":{...}}` | `emit:events` | Custom event. The server namespaces it to `plugin.<name>.scored` and fans it out to matching webhooks. |
| `{"op":"broadcast","room":"lobby","text":"..."}` | `rooms:broadcast` | Sends a `TextMessage` into the room. Receivers see the **nil session id** (`0000…`) as sender — i.e. "the server". |
| `{"op":"log","level":"info","message":"..."}` | — | Writes into the server log (`info`/`warn`/`error`). |

Anything your process writes to **stderr** also lands in the server log as
a warning line tagged `[plugin:<name>]` — handy for stack traces.

Correlation ids: the server generates its own ids (`srv-N`) for messages to
you; just echo them back. Clients use their own ids on the wire — the
server maps between the two, you never see the client's.

## 5. Hooks

| Hook | Kind | Event payload | `allow: false` means |
| :-- | :-- | :-- | :-- |
| `auth` | veto | `{username, role, remote}` | Login rejected — client gets `AuthFailure {"error":"forbidden"}`. |
| `text` | veto | `{room, text, sender:{username, role, session}}` | Message **silently dropped** (not routed). Notify the sender yourself via `broadcast` if desired. |
| `file` | veto | `{room, meta, sender}` (`meta` = parsed FileInit JSON or null) | The `FileInit` is dropped; the transfer never starts. |
| `presence` | notify | `{room, status, sender}` | — |
| `join` | notify | `{room, old_room, sender}` | — |
| `leave` | notify | `{room, sender}` | — |
| `tool_before` | veto | `{tool, plugin, args, caller}` | Call rejected with `tool_forbidden`. |
| `tool_after` | notify | `{tool, plugin, caller, ok, latency_ms}` | — |
| `shutdown` | notify | `{}` | Server is stopping. |

Veto semantics: every plugin registered on a hook is asked; **any**
`allow: false` blocks the action. If a plugin is down or does not answer
within `hook_timeout_ms`, its `hook_failure_policy` decides (`allow` =
fail-open, `deny` = fail-closed). Denies are counted per plugin
(`hook_denies` metric).

Performance note: hook dispatch happens on the message path, but only when
at least one plugin registered that hook — an idle server pays nothing.
Keep veto handlers fast (they gate chat latency); the 500 ms default
timeout is a ceiling, not a target.

## 6. Lifecycle

```
boot ──▶ scan PLUGINS_DIR ──▶ validate manifest ──▶ spawn ──▶ running
                                                        │ crash
                                                        ▼
                              restart with backoff 1s, 2s, 4s … max 30s
                              after 5 crashes ──▶ errored (gives up)
```

- **Load**: at boot, every subdirectory of `PLUGINS_DIR` containing a
  `plugin.json`. Duplicate plugin names or tool names are rejected.
- **Crash**: pending tool calls fail with `tool_failed: plugin exited`;
  the supervisor restarts with exponential backoff. After five crashes the
  plugin lands in `errored` with the reason in `last_error`.
- **Admin control** (see [admin-api.md](admin-api.md)):
  `POST /admin/v1/plugins/:name/disable` (graceful shutdown op, then kill),
  `/enable` (fresh start, resets the crash counter),
  `/reload` (re-reads `plugin.json` from disk and restarts — the way to
  pick up manifest or code changes without a server restart).
- **Server shutdown**: every plugin receives the `shutdown` hook
  notification and the `shutdown` op, then a kill after the grace period.

## 7. Calling tools from clients

Native packets (preferred — see [../spec/09-extensions.md](../spec/09-extensions.md)):

- `ToolCall (0x0070)`: `{"id":"<≤64 chars>","tool":"...","args":{...}}`
- `ToolResult (0x0071)`: `{"id","tool","ok":true,"result":...}`
- `ToolError (0x0072)`: `{"id","tool","ok":false,"error":{"code","message"}}`

Replies go **only to the caller**, never broadcast. Error codes:
`tool_not_found`, `tool_invalid_args`, `tool_rate_limited`,
`tool_timeout`, `tool_forbidden`, `tool_failed`.

Text fallback for clients without tool packets: send a `TextMessage`
whose payload is `TOOL:` + the ToolCall JSON; the reply is a `TextMessage`
`TOOLRESULT:` + the ToolResult/ToolError JSON, sent only to the caller.

SDK surface:

```js
// Node.js
await client.listTools();                    // system.list_tools under the hood
await client.callTool('echo.say', { text: 'hi' });   // throws Error with .code on ToolError
```

```python
# Python
client.list_tools()
client.call_tool('echo.say', {'text': 'hi'})   # raises on error
```

```php
// PHP
$client->listTools();
$client->callTool('echo.say', ['text' => 'hi']);
```

`system.list_tools` is a built-in handled by the server itself; it returns
`{"tools":[{name, description, schema, plugin}, …]}` including itself.

## 8. Testing and observability

- **Integration suite**: `bash tests/integration/run.sh` boots a server
  with the bundled example plugins and runs `tools_plugins.mjs`
  (discovery, roundtrip, error contract, fallback, moderation veto). Add
  cases for your plugin there or drive it with a five-line Node SDK
  snippet (`connect → authenticate → callTool`). The `adatp-cli` test tool
  does not speak tool packets — use an SDK.
- **Metrics**: per plugin — `calls`, `errors`, `restarts`,
  `avg_latency_ms`, `hook_denies`, `last_error` — visible in
  `GET /admin/v1/plugins` and on the Silo → PLUGINS cards.
- **Logs**: your `log` ops and stderr appear in the server log and the
  Silo → LOGS live stream, tagged `[plugin:<name>]`.

## 9. Security model

- **Default-deny**: no permission, no capability. Grant the minimum.
- **Process boundary**: plugins run as child processes of the server user.
  They are isolated from the server's memory, but not sandboxed from the
  OS — treat plugin code with the same trust as server code, and review
  third-party plugins before deploying them.
- **What plugins see**: hook events carry *decrypted* payloads (the server
  routes plaintext internally). The `auth` hook receives `username` and
  `role` — **never the password**; credential material stays inside the
  server's auth driver.
- **Resources**: there are no built-in CPU/memory quotas. Use OS-level
  controls (cgroups, ulimits, containers) for untrusted workloads, and
  keep `timeout_ms` / `rate_limit_per_min` tight.

## 10. Versioning and compatibility

- Your manifest `version` is yours; the server does not interpret it.
- The `init` message carries `server_version` — gate features on it if you
  need to.
- Forward compatibility: **ignore unknown ops and unknown fields**. The
  server does the same with yours, so additive protocol evolution never
  breaks a well-behaved plugin.

## 11. The bundled examples

Both live in `server/plugins/` and load automatically in the default dev
setup.

**`echo`** (`permissions: ["tools", "emit:events"]`) — the minimal
reference: one tool (`echo.say` with a `text`/`uppercase` schema), plus an
`emit_event` per call (`plugin.echo.echoed`) you can watch arrive at a
webhook receiver.

**`moderation`** (`permissions: ["tools", "hooks:text", "rooms:broadcast"]`)
— the veto reference. Walkthrough of its text hook:

1. Manifest registers `"hooks": ["text"]` with `hook_timeout_ms: 400` and
   `hook_failure_policy: "allow"` (chat stays up if the plugin dies).
2. On every routed text message the server sends
   `{"op":"hook","hook":"text","event":{room, text, sender…},"id":"srv-N"}`.
3. The plugin checks the text against its blocklist (`words.json` next to
   the script, or built-in defaults) and answers
   `{"op":"hook_result","id":"srv-N","allow":false}` on a match.
4. The server drops the message — the room never sees it — and the plugin
   logs which word matched.
5. The same list is queryable without sending anything via the
   `moderation.check` tool: `{"text": "..."} → {allowed, matched[]}`.

The integration suite proves the drop behavior end to end (a blocked
message never reaches the second client; a clean marker sent afterwards
does).
