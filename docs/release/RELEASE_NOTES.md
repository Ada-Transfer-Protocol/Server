# AdaTP v1.0.0

First public release of the Ada Transfer Protocol: a lightweight binary
realtime protocol for chat, voice, file transfer, game state and AI agents —
over a single WebSocket.

## Highlights

- **One transport, one port.** WebSocket-only data plane on `:3000` (`/ws`),
  one 45-byte-framed packet per binary message. The pre-1.0 raw-TCP listener
  is gone (`docs/legacy.md`).
- **Real security posture.** Credential verification on every connection
  (file/api/none drivers, fail-closed), X25519 → HKDF-SHA256 → AES-256-GCM
  session encryption interoperable across all six SDKs, and an honest
  security spec: this is transport encryption, not E2E — run `wss://` in
  production (`docs/spec/08-security.md`).
- **Fully specified.** RFC-style normative pack (`docs/spec/00…11` +
  appendices) with 9 deterministic golden vectors replayed by three
  independent implementations (Rust, Node.js, Python).
- **Plugin/tool platform.** Process-isolated plugins in any language
  (NDJSON over stdio) with default-deny permissions, JSON-Schema-validated
  tools (timeouts, rate limits, correlation ids), veto/notify hooks, crash
  auto-restart, and bundled `echo` + `moderation` examples.
- **Webhooks.** HMAC-SHA256-signed deliveries with retries, per-endpoint
  circuit breaker, SSRF guards, event catalog and delivery audit.
- **Operator experience.** Token-protected admin plane (`/admin/v1`) and the
  embedded **Silo Panel** (`/silo`): live KPIs, throughput chart,
  connections (kick), rooms, SSE log tail, webhook & plugin managers, drain.
- **GameState (0x0050).** First-class room-routed shared state with SDK
  APIs and a two-browser tic-tac-toe demo (`demos/game-lobby`).
- **Six SDKs at 1.0.0.** Browser JS, Node.js, Python, PHP, C (dependency-free
  RFC 6455 client) and Arduino/ESP32.
- **Portable & operable.** Non-root Docker image with a built-in
  healthcheck probe, tarball builder with SHA256SUMS, CI matrix
  (Linux/macOS), 20-file production runbook portal, graceful SIGTERM/SIGINT
  drain.

## Breaking changes (pre-1.0 wire renumbering)

| Was | Now |
| :-- | :-- |
| Ping/Pong `0x0070/71` | `0x0080/81` |
| Video family `0x0050-54` | `0x0090-94` (reserved) |
| — | GameState `0x0050`, ToolCall/Result/Error `0x0070-72` |

All SDKs in this release already speak the new map. Pre-1.0 raw-TCP clients:
see the migration note in `docs/legacy.md`.

## Verification

61 live integration assertions, tri-implementation conformance, and a load
sanity run (≈3 900 deliveries/s at p99 8 ms on a laptop) — commands and
expected results in `docs/testing/README.md` and `docs/release/V1_VERIFY.md`.

## Artifacts

- `adatp-server-1.0.0-<os>-<arch>.tar.gz` + `SHA256SUMS`
  (layout: `docs/release/ARTIFACTS.md`) — verify checksums before running.
- Docker: build from the repo (`docker build -t adatp-server:1.0.0 .`).

## Known limitations (honest)

Single-node (no clustering), at-most-once ephemeral messaging, no
Prometheus exporter yet, `file` auth driver is dev-only plaintext, and the
AdaTP handshake alone does not stop an active MITM — TLS in front is
required. Full candid matrix: `docs/enterprise/README.md`.
