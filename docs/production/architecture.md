# Production Architecture

## Topology

One AdaTP process, one port. TLS terminates at the edge; the origin speaks
plain HTTP/WebSocket on 3000.

```
                       Internet
                          │ wss:// (443, TLS)
              ┌───────────▼────────────┐
              │  Edge: LB / Cloudflare │   TLS termination, WS pass-through
              │  or nginx / caddy      │   health check → GET /readyz
              └───────────┬────────────┘
                          │ ws:// + http:// (3000)
        ┌─────────────────▼──────────────────┐
        │           adatp-server             │
        │                                    │
        │  /ws        ── data plane          │
        │  /healthz   ── liveness            │
        │  /readyz    ── readiness (drain)   │
        │  /api/*     ── metrics (x-api-key) │
        │  /admin/v1  ── control (token)     │
        │  /silo      ── operator UI         │
        └──┬────────────┬───────────┬────────┘
           │ stdio      │ SQLite    │ HTTPS egress
   ┌───────▼──────┐  ┌──▼───────┐  ┌▼──────────────┐
   │ plugin procs │  │ adatp.db │  │ webhook       │
   │ (child, one  │  │ api_keys │  │ endpoints     │
   │  per plugin) │  │ webhooks │  │ (your systems)│
   └──────────────┘  └──────────┘  └───────────────┘
```

## Data plane vs control plane

| Plane | Paths | Blocking behavior |
| :-- | :-- | :-- |
| Data | `/ws` — packets, rooms, voice, files, GameState | Hot path. Per-connection outbound queues (256); a slow consumer loses messages (counted in `dropped_messages`) instead of stalling the room. |
| Control | `/api/*`, `/admin/v1/*`, `/silo`, webhook delivery, plugin hooks | Never sits between two clients' packets. Webhook delivery is an async queue with its own workers; a dead endpoint cannot slow chat or voice. The only control-plane code on the data path is plugin *veto hooks* (auth/text/file/tool), each bounded by the plugin's `hook_timeout_ms` (default 500 ms). |

## Process model

- **Server**: one Tokio process. Every client connection is an independent task.
- **Plugins**: separate OS child processes speaking NDJSON on stdio. A plugin
  crash cannot take the server down; the server restarts it with backoff and
  gives up (state `errored`) after 5 crashes. See [plugins-ops.md](./plugins-ops.md).
- **Webhooks**: an in-process dispatcher (queue capacity 1024, 2 workers,
  retries, per-endpoint circuit breaker) making outbound HTTPS requests.
  See [webhooks-ops.md](./webhooks-ops.md).

## State model

| State | Where | Survives restart? |
| :-- | :-- | :-- |
| Connections, rooms, presence | Memory | **No** — clients must reconnect and re-join |
| Messages, files, voice | Never stored | **No** — at-most-once delivery, by design |
| API keys | SQLite (`api_keys`) | Yes |
| Webhook endpoints | SQLite (`webhooks`) | Yes |
| Users (file driver) | `users.json` | Yes (file) |
| Plugins | `PLUGINS_DIR` on disk | Yes (files); runtime state (enabled/disabled toggles) resets to manifest defaults on restart |
| Admin token | `ADMIN_TOKEN` env | Only if you set it (generated tokens change per boot) |

Full delivery-semantics discussion: [`../architecture/reliability.md`](../architecture/reliability.md).

## Failure domains

| Failure | Blast radius | Detection | Reference |
| :-- | :-- | :-- | :-- |
| Server process dies | All sessions drop; clients reconnect to a restarted process with empty rooms | Edge health check on `/readyz`, `/healthz` probe, systemd/K8s restart | [incident-runbook.md](./incident-runbook.md) |
| Auth backend (api driver) down | New logins rejected (`auth_unavailable`, fail-closed); existing sessions unaffected | auth failures in logs / `auth.failure` webhook events | [auth-providers.md](./auth-providers.md) |
| One plugin crashes | Its tools error (`tool_failed`); veto hooks follow `hook_failure_policy` (default fail-open) | `restarts`/`last_error` in `/admin/v1/plugins`, Silo PLUGINS tab | [plugins-ops.md](./plugins-ops.md) |
| Webhook endpoint down | Deliveries retried then dropped for that endpoint; breaker opens 60 s; nothing else affected | audit log, breaker LED in Silo | [webhooks-ops.md](./webhooks-ops.md) |
| SQLite file lost | API keys + webhook configs gone; data plane keeps running with already-loaded state until restart | `/readyz` DB probe on restart | [backup.md](./backup.md) |
| Edge/TLS down | Total outage from the client's view | Edge provider monitoring | [tls-cloudflare.md](./tls-cloudflare.md) |

## Network policy summary

Inbound: only the edge should reach 3000 (firewall or bind to a private
interface). Outbound: the server itself only dials out for two reasons —
`AUTH_API_URL` verification calls and webhook deliveries (SSRF-guarded,
public addresses only). Plugins are arbitrary processes: constrain their
egress at the OS/container level if you run third-party ones.
