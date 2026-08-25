# Silo Panel — Operator Runbook

Silo is the SCADA-style control room embedded in the server binary at
`/silo`. Everything it displays and every action it takes goes through the
token-protected [admin API](../platform/admin-api.md) — the panel has no
privileges of its own.

## Access control

- Login = the `ADMIN_TOKEN`. It is stored in the browser's
  `sessionStorage` for the tab's lifetime; LOCK (top right) clears it.
- The token authorizes **everything** (kick, drain, plugin control,
  webhook secrets at creation). Treat panel access = root on the realtime
  layer.
- Do not expose `/silo` or `/admin/v1` on the public hostname. Serve them
  on an internal/VPN vhost ([tls-cloudflare.md](./tls-cloudflare.md) has
  deny + internal-vhost blocks) and put your normal SSO/VPN in front as a
  second factor — the token alone is a single secret.
- If the token was never set, the server generated one and printed it to
  the log at boot (`warn`). That token changes every restart — set
  `ADMIN_TOKEN` properly before relying on the panel.

## Daily / per-shift checks (OVERVIEW)

| Check | Healthy | Investigate when… |
| :-- | :-- | :-- |
| Status LED (header) | green | amber = draining — is a drain left engaged? |
| ACTIVE CONNECTIONS | your normal band | sudden drop = edge/TLS problem; climb = launch or abuse |
| DROPPED MSGS | flat (ideally 0) | any steady climb — slow consumers / oversized rooms ([sizing.md](./sizing.md)) |
| THROUGHPUT chart | matches traffic | RX without TX (or inverse) = routing anomaly worth a look |
| PLUGINS m/n | m = n | any not-running plugin — PLUGINS tab |
| WEBHOOKS a/t | a = t | paused endpoints you didn't pause |
| LB HINTS capacity | < 70 % | approaching 100 % — capacity plan |

## Tab actions in incidents

| Situation | Tab | Action |
| :-- | :-- | :-- |
| Abusive/broken client | CONNECTIONS | KICK (server sends `Disconnect`, closes) — the client may reconnect; pair with credential revocation in your IdP for a real ban |
| Suspicious login pattern | LOGS | filter visually for `Auth failure` bursts; source IP is in each line |
| Planned maintenance | SETTINGS | ENGAGE DRAIN → watch OVERVIEW connections fall → do the work → RELEASE DRAIN ([upgrade-rollback.md](./upgrade-rollback.md)) |
| Runaway/broken plugin | PLUGINS | DISABLE first (stops its process; tools start returning `tool_failed`), read `last_error`, RELOAD after fixing |
| Webhook target flooding logs | WEBHOOKS | PAUSE the endpoint (breaker LED red = it's already open); resume after their incident |
| Updated users.json (file driver) | SETTINGS | RELOAD USER FILE |

## Live-binding proof (acceptance check)

After any deployment, prove the panel is real, not cached:

1. Open OVERVIEW, note ACTIVE CONNECTIONS.
2. From a shell: `cargo run -p adatp-cli -- -a <host>:3000 -u user1 -p <pw>`
3. The counter increments within one 2 s poll; the LOGS tab shows the auth
   line as it happens.

(The CI equivalent lives in `tests/integration/admin_webhooks.mjs` —
"overview counter moves when a client connects".)

## Operational notes

- Data refreshes every 2 s per tab; LOGS is push (SSE). Both survive
  reverse proxies — for nginx disable buffering on the SSE path
  ([tls-cloudflare.md](./tls-cloudflare.md)).
- The panel is stateless: closing the tab changes nothing server-side.
  A drain engaged from Silo persists until released (or restart).
- Webhook signing secrets are displayed **exactly once**, at creation, in
  the WEBHOOKS tab output line — copy them to your secret store in that
  moment ([webhooks-ops.md](./webhooks-ops.md)).
- Multiple operators can use Silo concurrently; last action wins. There is
  no audit trail of *which operator* acted (single shared token) — if you
  need per-operator accountability, front the internal vhost with an
  authenticating proxy that logs identities alongside timestamps, and
  correlate with the admin API access log lines.
