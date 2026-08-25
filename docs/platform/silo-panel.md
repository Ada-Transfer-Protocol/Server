# Silo Panel — Operator Guide

The Silo Panel is AdaTP's built-in operator UI: a factory/SCADA-style
control room served by the server itself at **`/silo`**. The three static
files (HTML/CSS/JS) are embedded into the binary at compile time — a
single executable ships the whole control plane, no separate frontend
deployment, no build step at runtime.

Everything Silo shows or changes goes through the token-protected
[admin API](admin-api.md). It renders **live server state, not mocks** —
the integration suite asserts this (§ Live-binding proof).

---

## Opening and logging in

1. Browse to `http://<host>:<port>/silo` (e.g. `http://127.0.0.1:3000/silo`).
2. Enter the **admin token** — the `ADMIN_TOKEN` environment variable, or
   the per-run token printed in the server log when the variable is unset.
3. `AUTHORIZE`. The token is kept in `sessionStorage` (survives reloads,
   dies with the tab). A wrong token shows `ACCESS DENIED`; any later 401
   locks the panel again. `LOCK` (top right) clears the token manually.

## Tab-by-tab tour

### OVERVIEW

- **KPI tiles**: ACTIVE CONNECTIONS, ROOMS, UPTIME, RX RATE, TX RATE,
  DROPPED MSGS (backpressure drops — should stay 0), PLUGINS
  (running/total), WEBHOOKS (active/total).
- **Throughput chart**: the last 60 seconds of RX (cyan) and TX (amber)
  bytes/s, one sample per second, drawn from `GET /admin/v1/load`.
- **Load balancer hints**: the `GET /lb-hints` line — healthy/draining,
  connections vs `MAX_CONNECTIONS`, capacity %.
- The header LED is green in normal operation and **amber while
  draining**.

### CONNECTIONS

Live table of authenticated sessions: id, user, role, room, remote
address, connect time — plus a **KICK** button per row
(`DELETE /admin/v1/connections/:id`; the client receives a graceful
`Disconnect`).

### ROOMS

All rooms with member counts, busiest first. Rooms exist only while
occupied, so an empty server shows none (or just `global`).

### LOGS

The live server log, streamed over SSE (`/admin/v1/logs/stream`) on top of
the last 200 buffered lines — the same lines operators see on stderr,
color-coded by level (WARN amber, ERROR red). The view keeps the last ~600
lines and auto-scrolls. Plugin output appears here tagged
`[plugin:<name>]`.

### WEBHOOKS

- **REGISTER ENDPOINT** form: URL, event filters (comma-separated;
  empty = `*`), description. On success the panel prints the endpoint id
  and the **signing secret — displayed exactly once**; copy it
  immediately, listings never show it again.
- **ENDPOINTS** table: status LED (green active · amber paused ·
  **red = circuit breaker open**), URL, filters, delivered/failed/skipped
  counters, and per-row actions — **TEST** (queues a signed
  `webhook.test`), **PAUSE/RESUME**, **DEL**.
- **DELIVERY AUDIT**: the most recent delivery outcomes (time, endpoint,
  event, outcome, HTTP status, attempt) from the 256-entry audit ring.

Consumer-side details (signatures, retries, SSRF):
[WEBHOOK_DEVELOPMENT.md](WEBHOOK_DEVELOPMENT.md).

### PLUGINS

One card per plugin: state (RUNNING/DISABLED/ERRORED), version,
description, tool and hook tags, metrics (calls, errors, restarts, average
latency) and `last_error` when present. Actions: **ENABLE / DISABLE /
RELOAD** (reload re-reads `plugin.json` from disk — the hot path for
plugin development, see [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md)).

### SETTINGS

- **ENGAGE DRAIN / RELEASE DRAIN** — toggles load-balancer drain:
  `/readyz` turns 503 and new connections are refused while existing
  sessions continue. (The disconnect-existing variant is API-only:
  `POST /drain {"enabled":true,"disconnect_clients":true}`.)
- **RELOAD USER FILE** — re-reads `users.json` for the `file` auth driver.
- **NON-SECRET CONFIGURATION** — the `GET /config` dump (no tokens or
  passwords, ever).

## Live-binding proof

Silo renders real state. To see it (and how CI asserts it):

1. Open OVERVIEW; note ACTIVE CONNECTIONS.
2. In a terminal, connect a real client:

   ```bash
   cd server && cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
   ```

3. Within one 2-second poll the tile increments; the connection appears
   under CONNECTIONS; `auth.success` lands in LOGS (and at any subscribed
   webhook). Disconnect and the counter falls back.

The automated version of this walk-through lives in
`tests/integration/admin_webhooks.mjs` — assertion
*"overview counter moves when a client connects (Silo live-binding
proof)"* — and runs in every full integration pass.

## Operational notes

- **Refresh model**: tables and tiles poll every 2 s; LOGS is push (SSE).
  A closed SSE stream (proxy timeout, restart) reattaches when you
  revisit the LOGS tab.
- **Token transport**: header `x-admin-token` for all calls; the SSE
  stream alone uses `?token=` (EventSource cannot set headers). Serve
  `/silo` over TLS in production so neither leaks.
- **Reverse proxies**: plain GET/POST + SSE — works behind nginx/
  Cloudflare; make sure your proxy does not buffer `text/event-stream`
  (nginx: `proxy_buffering off` for `/admin/v1/logs/stream`).
- **Multiple operators**: read views are safe concurrently; actions
  (kick, drain, plugin toggles) apply last-writer-wins.
- **Changing the UI**: the panel's source lives in `server/server/silo/`
  and is embedded via `include_str!` — rebuild the server to ship UI
  changes.
