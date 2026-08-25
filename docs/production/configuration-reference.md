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

Driver details and backend examples: [auth-providers.md](./auth-providers.md).

## Limits & timeouts

| Variable | Default | Type | Effect |
| :-- | :-- | :-- | :-- |
| `MAX_FRAME_BYTES` | `1048576` (1 MiB) | usize | Maximum accepted AdaTP payload. Oversized frames close the connection (`frame_too_large`). Also caps the WS message size (+4 KiB header slack). |
| `IDLE_TIMEOUT_SECS` | `90` | u64 | Connections with no inbound traffic for this long are dropped. The server sends WS protocol pings every 30 s; any conforming client stays alive automatically. |
| `MAX_CONNECTIONS` | `10000` | usize | **Reporting hint only** — surfaces in `/admin/v1/lb-hints` as capacity; the server does not refuse connections above it. |

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

ADMIN_TOKEN=<from-secret-store>
```

Verify what the server actually loaded (non-secret view):

```bash
curl -s -H "x-admin-token: $ADMIN_TOKEN" http://127.0.0.1:3000/admin/v1/config
```
