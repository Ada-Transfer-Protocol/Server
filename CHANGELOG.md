# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0] - 2026-08-25 — first public release

> Versioning note: pre-release internal builds were labeled "2.0.0". The
> public line starts at **1.0.0**; wire `version` byte is `1`.

### Added (platform)
- **Plugin/tool platform**: process-isolated plugins (any language, NDJSON
  over stdio) with manifest validation, default-deny permissions, JSON-Schema
  argument validation, per-tool timeout + rate limits, correlation ids,
  ToolCall/ToolResult/ToolError packets (0x0070-72) + `TOOL:` text fallback,
  veto/notify hooks (auth, text, file, presence, join/leave, tool
  before/after, shutdown), crash auto-restart with backoff, per-plugin
  metrics, bundled `echo` and `moderation` examples, `system.list_tools`.
- **Webhooks**: multi-endpoint, HMAC-SHA256-signed deliveries, async queue
  with retries (exp. backoff) and per-endpoint circuit breaker, SSRF guards,
  event catalog (auth/room/file/tool/server + plugin custom events),
  delivery audit log, local receiver tool.
- **Admin control plane `/admin/v1`**: token-authenticated overview,
  connections (+kick), rooms, live log stream (SSE + ring buffer),
  non-secret config, load series, LB hints, drain, webhook CRUD/test/audit,
  plugin enable/disable/reload, user-file reload.
- **Silo Panel**: SCADA-style operator UI embedded in the binary at `/silo`,
  live-bound to the admin API (KPIs, throughput chart, connections, rooms,
  logs, webhooks manager, plugins manager, drain controls).
- **GameState (0x0050)**: first-class room-routed state packets + SDK APIs
  + `demos/game-lobby` tic-tac-toe demo.
- **Conformance**: deterministic golden vectors replayed by Rust, Node.js
  and Python; integration suites (61 assertions); load test tool; CI.
- Docker image (non-root, self-healthcheck), portable tarball builder with
  SHA256 checksums.

### Changed (breaking, pre-1.0)
- Message-type renumbering: Ping/Pong 0x0070/71 → **0x0080/81**; video
  family 0x0050-54 → **0x0090-94** (reserved); 0x0050 is now GameState and
  0x0070-72 are the tool packets.
- `adatp-cli` admin binary renamed to **`adatp-admin`** (the protocol test
  tool keeps the `adatp-cli` name).

## [1.0.0-rc] — transport & auth unification

### Changed
- **Single transport (breaking):** the data plane is now WebSocket-only on
  `PORT` (default **3000**), endpoint `/ws`, one AdaTP packet per binary
  message. The raw-TCP listener on `:8444` was removed — see
  `docs/legacy.md` in the workspace root for the migration note.
- **Real authentication:** `AuthRequest` is now verified against a driver
  (`AUTH_DRIVER=file|api|none`). Failures answer `AuthFailure` with a JSON
  error; three failures close the connection; unavailable backends fail
  closed. Unauthenticated connections cannot join rooms or send traffic.
- **Real secure handshake:** `HandshakeInit` is answered with a genuine
  ephemeral X25519 key (previously a mock all-zero key), enabling the
  documented HKDF-SHA256 + AES-256-GCM session encryption end to end with
  the Node.js, Python, PHP, C and Arduino SDKs.
- **Room registry:** connections/rooms are tracked in a real registry
  (`JoinRoom` → `RoomJoined` confirmation, presence JOIN/LEAVE broadcasts,
  per-recipient re-encryption for secure sessions).
- `adatp-cli` test tool now connects over WebSocket (`-a host:port` or a
  full `ws(s)://` URL).

### Added
- `/healthz` and `/readyz` endpoints; `MAX_FRAME_BYTES` and
  `IDLE_TIMEOUT_SECS` limits; WS keep-alive pings; graceful shutdown that
  notifies clients with `Disconnect`.
- Integration test suite at `tests/integration/` (plaintext + encrypted
  end-to-end: auth, join, text roundtrip, room isolation).

### Fixed
- `adatp-cli` (API-key manager binary) no longer requires a live database at
  compile time (`sqlx::query!` macros replaced with runtime queries).

## [2.0.0] - 2024-01-28
### Added
- **Universal Linux Installer:** `setup.sh` now supports automated installation on Ubuntu, Debian, RHEL, CentOS, Fedora, Arch, and Alpine.
- **SSH Welcome Screen (MOTD):** Professional dashboard displaying system status, IP, ports, and developer credits upon login.
- **Authentication:** Added `AUTH_API_URL` support for external Webhook-based authentication alongside internal SQLite auth.
- **Admin CLI:** Enhanced `adatp` tool with `--username` and `--password` flags for secure authenticated connections.
- **Uninstaller:** Added `tools/uninstall.sh` for complete system cleanup.
- **Documentation:** Comprehensive `README.md` and `PROTOCOL_SPEC.md` updates.

### Changed
- **Service Name:** Systemd service renamed to `adatp-server` (aliased commands updated).
- **Default Config:** `Cargo.lock` is now tracked for reproducible builds.
- **Dependency Management:** Support for offline/vendored builds (with automatic fallback to online).
- **SDK Links:** Added references to Official SDKs for JS, Node.js, Python, PHP, and C.

### Fixed
- **Installation:** Resolved persistent caching issues with installation scripts via version query params.
- **MOTD:** Fixed bash syntax errors in welcome screen generation (heredoc/variable expansion).
- **CLI:** Fixed default connection port from 8443 to 3000 to match server default.
