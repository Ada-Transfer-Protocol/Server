# Configuration Reference

Configuration is environment variables only. A `.env` file in the server's
working directory is loaded at startup (dotenv). **No hot reload**: every
change below requires a restart — the single exception is the file driver's
user list, reloadable at runtime via `POST /admin/v1/users/reload`.

## Core

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `HOST` | `0.0.0.0` | addr | Bind address. Use `127.0.0.1` behind a local reverse proxy. Fallback name: `SERVER_HOST`. |
| `PORT` | `3000` | u16 | The single listen port (WS `/ws` + HTTP). Fallback name: `SERVER_PORT`. |
| `RUST_LOG` | `info` | level | `error`·`warn`·`info`·`debug`·`trace` (plain level filter, no per-module directives). Production: `warn` — `info` logs every auth/join. |

## Authentication

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `AUTH_DRIVER` | `file` | enum | `file` \| `api` \| `none`. Unknown value = startup panic (fail-fast). Setting `AUTH_API_URL` with no `AUTH_DRIVER` selects `api`. |
| `AUTH_FILE_PATH` | `users.json` | path | User file for the `file` driver (plaintext passwords — **dev/demo only**). Falls back to `server/<path>` for workspace-root runs. |
| `AUTH_API_URL` | — | URL | External verifier for the `api` driver. `POST {username,password}` → `{authorized,user_id,role}`, 5 s timeout, **fail-closed**. Required when `AUTH_DRIVER=api`. |

**File driver is fail-closed at startup.** When `AUTH_DRIVER=file` (including the
default), the server **refuses to start** if no readable user file is found or the
file defines zero users — it will *not* silently come up with no valid logins or
with leftover demo credentials. Copy `server/users.example.json` to
`server/users.json` (git-ignored) and set real values, or choose `AUTH_DRIVER=api`
/ `AUTH_DRIVER=none` explicitly. The repository ships **no** `users.json`; the
`none` and `api` drivers never touch a user file.

Driver details and backend examples: [auth-providers.md](./auth-providers.md).

## Limits & timeouts

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `MAX_FRAME_BYTES` | `1048576` (1 MiB) | usize | Maximum accepted AdaTP payload. Oversized frames close the connection (`frame_too_large`). Also caps the WS message size (+4 KiB header slack). |
| `IDLE_TIMEOUT_SECS` | `90` | u64 | Connections with no inbound traffic for this long are dropped. The server sends WS protocol pings every 30 s; any conforming client stays alive automatically. |
| `MAX_CONNECTIONS` | `10000` | usize | **Hard cap on concurrent WebSocket connections.** New connections above the cap are rejected at the HTTP upgrade with `503 max connections reached`. The reservation is released when the connection closes (no leak on a rejected/abandoned upgrade). Also surfaces in `/admin/v1/lb-hints` as capacity. |
| `MSG_RATE_LIMIT` | `200` | u32 | Per-connection inbound message rate limit in **messages/second** (token bucket; the value is also the burst size). A connection that exceeds it is closed (`rate_limited`). `0` disables the limit. Applies to AdaTP binary frames; WS ping/pong keepalives are exempt. |

## Authorization (rooms)

Room joins are gated by two independent, real mechanisms enforced **before** a
join takes effect: this built-in config policy, and a plugin `join` **veto**
hook (a policy plugin that replies `allow:false` blocks the join — mirrors the
`auth` and `tool_before` veto hooks). Both default to permissive, so public
rooms keep working until an operator opts in.

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `ROOM_ALLOWLIST` | — (empty) | csv | Comma-separated room allowlist. When non-empty, only listed rooms may be joined; others are rejected (`room_not_allowed`). Empty = every room allowed. |
| `ROOM_PROTECTED_PREFIX` | — (unset) | string | Rooms whose name **starts with** this prefix require `ROOM_PROTECTED_ROLE`. Unset = no prefix is protected. Example: `admin-`. |
| `ROOM_PROTECTED_ROLE` | `admin` | string | Role a user must have to join a `ROOM_PROTECTED_PREFIX` room. Others are rejected (`room_forbidden`). |

A denied join returns an `AuthFailure` frame (`{"error":"room_not_allowed"}`,
`{"error":"room_forbidden"}`, or `{"error":"forbidden"}` for a plugin veto) and
leaves the connection open in its current room.

## Persistence & platform

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `DATABASE_URL` | `sqlite:adatp.db` | URL | SQLite for API keys + webhook endpoints. The file is created if missing. Put it on the persistent volume (`sqlite:/app/data/adatp.db` in Docker). |
| `PLUGINS_DIR` | `plugins` | path | Directory scanned at boot for `*/plugin.json`. Missing directory = platform idle (not an error). Falls back to `server/plugins`. |

## Control plane & security

| Variable | Default | Type | Effect / security notes |
| :-- | :-- | :-- | :-- |
| `ADMIN_TOKEN` | *generated* | secret | Token for `/admin/v1` + Silo. If unset, a random token is generated **per boot** and printed to the log at `warn` — fine for a first look, wrong for production. Set it from a secret manager; treat like a root password. |
| `ADATP_WEBHOOK_ALLOW_PRIVATE` | unset | bool (`1`/`true`) | Disables the webhook SSRF guard (allows loopback/private targets). **Development only — never set in production.** Startup logs a warning when active. |

Not configurable via env (compiled defaults): auth attempt limit (3),
pre-auth violation limit (10), per-connection outbound queue (256), WS ping
interval (30 s), webhook retry/breaker parameters, plugin restart backoff.
See [`../architecture/reliability.md`](../architecture/reliability.md).

## Example production .env

```env
HOST=127.0.0.1
PORT=3000
RUST_LOG=warn

AUTH_DRIVER=api
AUTH_API_URL=https://auth.internal.example.com/verify

DATABASE_URL=sqlite:/var/lib/adatp/adatp.db
PLUGINS_DIR=/var/lib/adatp/plugins

MAX_FRAME_BYTES=1048576
IDLE_TIMEOUT_SECS=90
MAX_CONNECTIONS=8000
MSG_RATE_LIMIT=200

ADMIN_TOKEN=<from-secret-store>
```

Verify what the server actually loaded (non-secret view):

```bash
curl -s -H "x-admin-token: $ADMIN_TOKEN" http://127.0.0.1:3000/admin/v1/config
```

## Authenticated handshake, backplane & publish (v1.2+)

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `ADATP_IDENTITY_PATH` | `adatp-identity.key` | path | File holding the server's long-term Ed25519 identity seed for the v2 authenticated handshake. Generated (0600) on first boot; its public key is what clients pin. Its fingerprint is logged at startup. |
| `ADATP_MIN_PROTOCOL_VERSION` | `1` | u8 | Minimum handshake version. **Set to `2` to require the authenticated v2 handshake** and reject v1 (the downgrade defense). Default stays `1` for back-compat until every SDK speaks v2. |
| `ADATP_BACKPLANE_URL` | — | URL | `redis://host:port` for the multi-node routing backplane (active-active rooms). Unset = single-node (in-process routing only). Fail-closed at boot if set but unreachable. |
| `ADATP_PUBLISH_SECRET` | — | secret | HMAC secret for the HTTP publish endpoint (`POST /publish`, see [publish-api.md](../platform/publish-api.md)). Unset = the endpoint is disabled (503). |

The `api` auth driver also forwards an optional `auth_string` in the POST body
(`{username, password, auth_string}`) so a client can authenticate with a single
token; the `file` driver matches `auth_string` against a user's optional `token`.
