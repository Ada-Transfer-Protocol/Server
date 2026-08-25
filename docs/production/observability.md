# Observability

What you can see, where it comes from, and — honestly — what v1 does not give
you yet.

## Surfaces

| Surface | Auth | Format | Use |
| :-- | :-- | :-- | :-- |
| `GET /healthz` | none | JSON | liveness (`{"status":"ok"}`) — the k8s liveness probe and the container `--healthcheck` both hit this |
| `GET /readyz` | none | JSON | readiness; `503 {"status":"draining"}` during drain, `503 {"status":"not_ready"}` if SQLite is unreachable — the k8s readiness probe |
| `GET /api/status` | `x-api-key` | JSON | service id, auth driver, connection count |
| `GET /api/metrics` | `x-api-key` | JSON | the counter snapshot (below) |
| `GET /admin/v1/overview` | admin token | JSON | the KPI snapshot Silo shows |
| `GET /admin/v1/load` | admin token | JSON | 60-sample, 1 Hz throughput/connection series |
| `GET /admin/v1/logs` | admin token | JSON | last ≤200 log lines (ring buffer, capacity 500) |
| `GET /admin/v1/logs/stream` | admin token (`?token=` allowed) | SSE | live log tail (what Silo → LOGS renders) |
| `GET /admin/v1/webhooks` + `/webhooks/audit` | admin token | JSON | per-endpoint stats + last 256 delivery outcomes |
| `GET /admin/v1/plugins` | admin token | JSON | per-plugin calls/errors/restarts/latency/last_error |
| stderr | — | text lines | the same log stream, for journald/Docker shipping |

Admin surfaces authenticate with the `x-admin-token` header (or `Bearer`, or
`?token=` for the SSE stream); the token is `ADMIN_TOKEN` — set it explicitly
(see [install-kubernetes.md](./install-kubernetes.md) /
[install-docker.md](./install-docker.md)) or the server generates a throwaway
one at boot.

## `GET /api/metrics` fields

A **small JSON counter set** (source: `server/src/metrics.rs` +
`server/src/api.rs`), served behind `x-api-key`:

```json
{
  "uptime_seconds": 86400,
  "active_connections": 412,
  "total_bytes_received": 123456789,
  "total_bytes_sent": 987654321,
  "avg_rx_speed_bps": 1428,          // lifetime average, not current rate
  "rooms": [ { "name": "lobby", "members": 12 } ],
  "dropped_messages": 0              // slow-consumer drops — watch this
}
```

For *current* rates use `/admin/v1/load`:

```json
{ "current": { "at_ms": 0, "connections": 412,
               "rx_bytes_per_s": 51234, "tx_bytes_per_s": 498112 },
  "series": [ ] }
```

## The three numbers worth alerting on

1. **`/readyz` != 200** for longer than your drain window → page.
2. **`dropped_messages` increasing** → fan-out exceeds consumers
   ([sizing.md](./sizing.md)); investigate the biggest rooms.
3. **auth failure rate** (log lines `Auth failure` / `auth.failure` webhook
   events) → credential stuffing or a broken IdP.

## Metrics format: honest gap (roadmap)

**Today `/api/metrics` is a small JSON counter set — there is no Prometheus
text endpoint and no OpenTelemetry.** A Prometheus-format exporter and
OpenTelemetry tracing are **roadmap**, not shipped: see the *Metrics* row of
the [enterprise readiness checklist](../enterprise/README.md) ("⚠️ JSON
endpoints + 60 s load series; **no Prometheus/OTel exporter yet**"). Until then,
two workable patterns:

- **JSON scraping**: Telegraf `inputs.http` / Prometheus `json_exporter` / a
  10-line cron against `/api/metrics`, mapping fields to gauges. All counters
  are monotonic except `active_connections`. Because `/api/metrics` requires an
  `x-api-key`, give your scraper a dedicated key.
- **Webhook-driven eventing**: register an endpoint for `auth.*`,
  `connection.closed`, `tool.called` and feed your analytics pipeline — push,
  not poll ([webhooks-ops.md](./webhooks-ops.md)).

There is no in-cluster `ServiceMonitor` target in v1 (no Prometheus endpoint to
scrape); the JSON-scraping bridge above is the supported path.

## Logs

- One stream, mirrored three ways: stderr (ship via journald/Docker), the
  in-memory ring (last 500 lines, `/admin/v1/logs`), and SSE for Silo.
- `RUST_LOG` sets a plain level: `error|warn|info|debug|trace`.
  Production: **`warn`** — at `info` every connection, auth and join is a line,
  which is useful in staging and noisy at scale.
- Plugin stderr is folded into the server log as `[plugin:<name>] …` at `warn`
  — crash stack traces show up there.
- Log lines are unstructured text with a structured JSON mirror on the admin
  surfaces (`{at_ms, level, target, message}`); there is no JSON formatter for
  the stderr stream in v1.

journald examples:

```bash
journalctl -u adatp-server -f
journalctl -u adatp-server --since -1h | grep -c "Auth failure"
```

## Webhook & plugin health

- `GET /admin/v1/webhooks` — `delivered`/`failed`/`skipped_breaker` and
  `breaker_open` per endpoint; audit shows per-delivery outcome, HTTP status,
  attempt number.
- `GET /admin/v1/plugins` — `restarts` climbing = crash loop; `avg_latency_ms`
  near a tool's `timeout_ms` = timeouts imminent; `last_error` carries the
  give-up reason after repeated crashes.

## Uptime probes

External monitoring should hit `GET /healthz` (cheap, no auth) *through the
edge* — that also validates TLS and the proxy. A deeper synthetic check is the
protocol probe:

```bash
cargo run -p adatp-cli -- -a wss://realtime.example.com/ws -u probe -p '<secret>'
```

run from cron/CI with a dedicated `probe` account — it exercises handshake,
encryption, and auth in one shot.

## Load & capacity testing

Latency/throughput are not observability counters — measure them with the
benchmark harness and record real numbers per environment. See
[benchmarks.md](./benchmarks.md) (`tools/loadtest/run-benchmark.sh`), which
also reads `dropped_messages` back from `/api/metrics` after a run.
