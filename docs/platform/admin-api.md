# AdaTP Admin API Reference (`/admin/v1`)

The admin control plane: everything the [Silo panel](silo-panel.md)
displays and does goes through these endpoints — there is no hidden
channel, so anything Silo can do, `curl` can do.

Base URL: `http(s)://<host>:<port>/admin/v1` (same single listener as the
data plane; see [ports](../deployment/ports.md)).

---

## Authentication

Every request must present the **admin token**:

- Header `x-admin-token: <token>` (preferred), or
- Header `Authorization: Bearer <token>`, or
- Query `?token=<token>` — provided for the SSE log stream, where
  `EventSource` cannot set headers. Avoid it elsewhere (tokens in URLs end
  up in proxy logs).

The token comes from the `ADMIN_TOKEN` environment variable. If unset, the
server **generates one per run and prints it to the log** at startup:

```
WARN  ADMIN_TOKEN not set — generated one for this run: 3f2c9a…
WARN  Set ADMIN_TOKEN in the environment to keep it stable.
```

Comparison is constant-time. A missing/wrong token yields:

```http
401 {"error": "invalid_admin_token"}
```

Common error shapes elsewhere: `404 {"error":"unknown_connection"}`,
`404 {"error":"unknown_webhook"}`, `400 {"error":"<message>"}`,
`400 {"error":"invalid_url","detail":"…"}`,
`500 {"error":"db_error","detail":"…"}`.

For the examples below:

```bash
export ADMIN_TOKEN=…            # your token
A() { curl -s -H "x-admin-token: $ADMIN_TOKEN" "$@"; }
```

---

## Observability

### `GET /overview`

Everything the Silo OVERVIEW tab shows, in one call.

```bash
A http://127.0.0.1:3000/admin/v1/overview
```

```json
{
  "service": "adatp-server",
  "version": "1.0.0",
  "uptime_seconds": 4211,
  "draining": false,
  "auth_driver": "file",
  "connections": { "active": 12, "dropped_messages": 0 },
  "rooms": 3,
  "traffic": { "total_bytes_received": 918273, "total_bytes_sent": 8812733 },
  "plugins": { "total": 2, "running": 2 },
  "webhooks": { "total": 1, "active": 1 }
}
```

### `GET /connections` · `DELETE /connections/:id`

```json
{ "connections": [ {
    "id": 7, "session_id": "8f0e4d2f9c371a2b3c4d5e6f00010203",
    "username": "user1", "role": "user", "room": "lobby",
    "remote": "203.0.113.9:52114", "connected_at_ms": 1787660000000 } ] }
```

`DELETE /connections/7` asks that connection to close gracefully (the
client receives `Disconnect "server_shutdown"`). `200 {"ok":true}` or
`404 {"error":"unknown_connection"}`.

### `GET /rooms`

```json
{ "rooms": [ { "name": "global", "members": 4 },
             { "name": "lobby",  "members": 8 } ] }
```

### `GET /logs` · `GET /logs/stream`

`/logs` returns the last 200 in-memory log lines:

```json
{ "lines": [ { "at_ms": 1787660000000, "level": "INFO",
               "target": "adatp_server::connection",
               "message": "Auth success for 203.0.113.9:52114: user1 (role user)" } ] }
```

`/logs/stream` is **Server-Sent Events** — one `data:` JSON per log line,
with keep-alives. Because `EventSource` cannot set headers, pass the token
as a query parameter:

```js
new EventSource(`/admin/v1/logs/stream?token=${encodeURIComponent(TOKEN)}`)
    .onmessage = (e) => console.log(JSON.parse(e.data));
```

```bash
curl -N "http://127.0.0.1:3000/admin/v1/logs/stream?token=$ADMIN_TOKEN"
```

The buffer holds ~500 lines; the stream is live from subscription time.

### `GET /config`

Non-secret runtime configuration — **never** tokens, keys, or passwords:

```json
{
  "host": "0.0.0.0", "port": 3000,
  "auth_driver": "file", "auth_file_path": "users.json",
  "auth_api_url_set": false,
  "database_url": "sqlite:adatp.db",
  "max_frame_bytes": 1048576, "idle_timeout_secs": 90,
  "plugins_dir": "plugins"
}
```

### `GET /load`

One sample per second, one minute of history (what Silo charts):

```json
{ "current": { "at_ms": 1787660000000, "connections": 12,
               "rx_bytes_per_s": 18211, "tx_bytes_per_s": 174220 },
  "series": [ … up to 60 samples … ] }
```

### `GET /lb-hints`

For load balancers and autoscalers (`MAX_CONNECTIONS` env, default 10000):

```json
{ "healthy": true, "draining": false,
  "connections": 12, "max_connections": 10000, "capacity_used_pct": 0 }
```

---

## Node operations

### `POST /drain`

```bash
A -X POST -H "content-type: application/json" \
  -d '{"enabled": true, "disconnect_clients": false}' \
  http://127.0.0.1:3000/admin/v1/drain
# → {"ok": true, "draining": true}
```

While draining: `/readyz` answers `503 {"status":"draining"}` and **new**
WebSocket connections are refused with 503; existing sessions continue
unless `disconnect_clients: true`, which also asks every connection to
close gracefully. `{"enabled": false}` releases the drain.

### `POST /users/reload`

Re-reads the `file` auth driver's user file without a restart:

```bash
A -X POST http://127.0.0.1:3000/admin/v1/users/reload
# → {"ok": true, "users": 6}      (400 for non-file drivers)
```

---

## Webhooks

Full consumer-side documentation: [WEBHOOK_DEVELOPMENT.md](WEBHOOK_DEVELOPMENT.md).

### `GET /webhooks`

```json
{ "webhooks": [ {
    "id": "6f9c…", "url": "https://app.example.com/hooks/adatp",
    "events": ["auth.*", "room.joined"], "is_active": true,
    "description": "ops pipeline", "created_at": "2026-08-25T12:00:00Z",
    "delivered": 41, "failed": 0, "skipped_breaker": 0,
    "breaker_open": false } ] }
```

Secrets are redacted in listings — shown exactly once at creation.

### `POST /webhooks`

```bash
A -X POST -H "content-type: application/json" \
  -d '{"url":"https://app.example.com/hooks/adatp",
       "events":["auth.*","room.joined"],
       "description":"ops pipeline"}' \
  http://127.0.0.1:3000/admin/v1/webhooks
# → {"ok": true, "id": "6f9c…", "secret": "b1946ac9…"}   ← store it now
```

`events` defaults to `["*"]`; `secret` is generated when omitted. The URL
must pass the SSRF guard or you get
`400 {"error":"invalid_url","detail":"host resolves to non-public address …"}`.

### `PATCH /webhooks/:id` · `DELETE /webhooks/:id` · `POST /webhooks/:id/test`

```bash
A -X PATCH  -H "content-type: application/json" -d '{"active": false}' …/webhooks/6f9c…
A -X DELETE …/webhooks/6f9c…
A -X POST   …/webhooks/6f9c…/test     # → {"ok":true,"queued":true}; sends webhook.test, no retries
```

### `GET /webhooks/audit`

Last 256 delivery outcomes:

```json
{ "audit": [ { "at_ms": 1787660000000, "endpoint_id": "6f9c…",
               "event": "room.joined", "outcome": "delivered",
               "status": 200, "attempt": 1 } ] }
```

`outcome` ∈ `delivered | retrying | failed | skipped_breaker | blocked_ssrf`.

---

## Plugins

Full developer-side documentation: [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md).

### `GET /plugins`

```json
{ "plugins": [ {
    "name": "moderation", "version": "1.0.0",
    "description": "Blocks text messages containing configured words…",
    "state": "running",
    "tools": ["moderation.check"], "hooks": ["text"],
    "permissions": ["tools", "hooks:text", "rooms:broadcast"],
    "calls": 12, "errors": 0, "restarts": 0,
    "avg_latency_ms": 3, "last_error": null } ] }
```

`state` ∈ `running | disabled | errored`.

### `POST /plugins/:name/enable` · `/disable` · `/reload`

```bash
A -X POST http://127.0.0.1:3000/admin/v1/plugins/moderation/disable
A -X POST http://127.0.0.1:3000/admin/v1/plugins/moderation/enable    # resets crash counter
A -X POST http://127.0.0.1:3000/admin/v1/plugins/moderation/reload    # re-reads plugin.json + restarts
```

Each returns `{"ok":true}` or `400 {"error":"<reason>"}`.

---

## Relationship to other HTTP surfaces

| Path | Auth | Purpose |
| :-- | :-- | :-- |
| `/ws` | AdaTP AuthRequest | The data plane. |
| `/healthz`, `/readyz` | none | Probes (readyz reflects drain + DB health). |
| `/api/status`, `/api/metrics` | `x-api-key` (SQLite `api_keys` table, managed by the `adatp-admin` binary) | Legacy lightweight metrics surface. |
| `/admin/v1/*` | admin token | This API. |
| `/silo` | static shell; its data calls use this API | Operator UI. |
