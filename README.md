# Ada Transfer Protocol (Server)

<p>
  <img src="https://img.shields.io/badge/AdaTP-v1.2.0-blueviolet?style=for-the-badge" alt="AdaTP v1.2.0">
  <img src="https://img.shields.io/badge/Built%20with-Rust-orange?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/License-MIT-green?style=for-the-badge" alt="MIT">
  <img src="https://img.shields.io/badge/Peers-MCU%20%E2%86%94%20Browser-0b3d91?style=for-the-badge" alt="MCU to Browser">
  <img src="https://img.shields.io/badge/Model-single--node-lightgrey?style=for-the-badge" alt="Single-node">
</p>
<p>
  <img src="https://img.shields.io/badge/Transport-WebSocket%20(axum%200.7)-0a7ea4?style=flat-square" alt="WebSocket over axum">
  <img src="https://img.shields.io/badge/Frame-45--byte%20binary-6f42c1?style=flat-square" alt="45-byte frame">
  <img src="https://img.shields.io/badge/Session%20crypto-X25519%20%E2%86%92%20AES--256--GCM-d63384?style=flat-square" alt="AES-256-GCM">
  <img src="https://img.shields.io/badge/Encryption-hop--by--hop%20(not%20E2E)-important?style=flat-square" alt="Hop-by-hop, not E2E">
  <img src="https://img.shields.io/badge/Runtime-Tokio%20async-000000?style=flat-square" alt="Tokio">
  <img src="https://img.shields.io/badge/Auth-fail--closed-critical?style=flat-square" alt="Fail-closed auth">
  <img src="https://img.shields.io/badge/Webhooks-HMAC%20signed%20%2B%20SSRF--guarded-fd7e14?style=flat-square" alt="Signed webhooks">
  <img src="https://img.shields.io/badge/Plugins-process--isolated-20c997?style=flat-square" alt="Process-isolated plugins">
  <img src="https://img.shields.io/badge/Conformance-golden%20vectors-2ea44f?style=flat-square" alt="Conformance vectors">
  <img src="https://img.shields.io/badge/SDKs-6%20repos-informational?style=flat-square" alt="6 SDK repos">
</p>

> **The only realtime protocol where a ~20 KB-RAM microcontroller is a
> first-class peer with the browser** — the same rooms, the same 45-byte
> frame, the same X25519/AES-GCM session crypto, and the same conformance
> vectors, running on an STM32 *and* in a Chrome tab.

**AdaTP (Ada Transfer Protocol)** is a realtime communication server written in
Rust: chat, voice (raw PCM), file transfer, shared game/robot state, an
extensible plugin/tool platform, signed webhooks, and a built-in operator
panel — all over **one WebSocket port**, in one self-contained binary. The
wire format is a lightweight **45-byte binary frame** (no SIP/WebRTC weight),
fully specified in [`docs/SPEC.md`](docs/SPEC.md) → [`docs/spec/`](docs/spec/)
and replayed by golden test vectors across independent implementations.

> ### Security, stated honestly (read before deploying)
>
> AdaTP's own encryption is **hop-by-hop transport encryption — NOT
> end-to-end.** The server **decrypts every packet to route it** and
> **re-encrypts it per recipient**, so it sees plaintext. The app-layer
> handshake is **not yet cryptographically authenticated**, so it does not stop
> an active man-in-the-middle by itself. **Therefore TLS termination at a
> reverse proxy (`wss://`) is REQUIRED in production** — TLS provides the real
> server-authentication and MITM protection today; AdaTP session crypto is
> defense-in-depth on top of it. Full details, threat model, and the roadmap to
> an authenticated key exchange: **[`docs/SECURITY_MODEL.md`](docs/SECURITY_MODEL.md).**

---

## Why AdaTP? The embedded-first thesis

Most realtime servers compete on latency benchmarks for browser and mobile
fan-out. AdaTP competes on a different, defensible axis: **the same protocol
that talks to a browser tab also runs on a microcontroller with kilobytes of
RAM** — same rooms, same crypto, same wire format, same conformance vectors on
both ends.

> **Why AdaTP?** *Because your device has 20 KB of RAM and nothing else fits
> there — while the same protocol still talks to a browser tab.*

**Target domains** — realtime messaging for **constrained devices**, not a
general-purpose "beat the incumbents on latency" play:

- **IoT & embedded** — sensors and actuators as first-class room peers.
- **Industrial & factory automation** — SCADA-style telemetry and control
  over a single auditable protocol (there is even a built-in
  [Silo operator panel](docs/platform/silo-panel.md)).
- **High-speed manufacturing telemetry** — many small frames, tight budgets.
- **Robotics, UAVs & drones** — shared state (`GameState`) and low-overhead
  signaling between fleet, ground station, and dashboards.

**The C / embedded SDK is the crown jewel**, not a sixth afterthought.
[SDK-C] and [SDK-ARDUINO-ESP32] implement the *complete* handshake — X25519 key
agreement, HKDF-SHA256, AES-256-GCM — on the device itself, so an MCU peers
with a Chrome tab through the exact same rooms and vectors. (Authoritative
memory-footprint numbers live in the SDK-C repository.)

**Honest about scale.** AdaTP v1 is **single-node**: rooms, connections, and
presence live in memory; only API keys and webhook endpoints persist (SQLite).
It is excellent for one well-sized node and edge-sharded tenants. **Massive
cross-node fan-out and clustering are a roadmap item** (a state backplane), not
a shipped feature — see [`docs/production/ha.md`](docs/production/ha.md) and
[`ROADMAP.md`](ROADMAP.md).

---

## What ships today

| Pillar | What is real in v1 |
| :-- | :-- |
| **Embedded reach** | The full X25519 → HKDF-SHA256 → AES-256-GCM session and the 45-byte frame run on the **C / Arduino-ESP32** SDKs, on-device — the same code path browsers use. |
| **Security** | Ephemeral **X25519 → HKDF-SHA256 → AES-256-GCM** session encryption (**hop-by-hop**, re-encrypted per recipient — [not E2E](docs/SECURITY_MODEL.md)); credential verification on every connection (**fail-closed**); HMAC-SHA256-signed webhooks with IPv4/IPv6 SSRF guards. |
| **Extensibility** | Process-isolated **plugins in any language** (NDJSON over stdio) exposing callable tools and default-deny policy hooks; a documented **45-byte** wire format with golden conformance vectors and **6 SDK repositories**. |
| **Operability** | One-line installer + `systemd` service, management CLI, embedded **Silo** operator panel, structured logging, `/healthz` + `/readyz` probes, graceful drain, and a full [production portal](docs/production/README.md). |
| **Concurrency** | **Tokio** async, one task per connection, a lock-free **DashMap** room registry, and zero-copy plaintext fan-out (encrypted sessions are re-encrypted per recipient). Best-effort, at-most-once delivery. |

**Platform highlights**

- **Plugins & tools** — process-isolated plugins (any language) expose callable
  tools and policy hooks. [Guide](docs/platform/PLUGIN_DEVELOPMENT.md)
- **Webhooks** — HMAC-signed event deliveries with retries, a circuit breaker,
  and SSRF guards. [Guide](docs/platform/WEBHOOK_DEVELOPMENT.md)
- **AI agents & devices** — raw 16 kHz PCM, tool calling, and `GameState` make
  agent and device integration first-class. [Guide](docs/developer/AI_AGENT_DEVELOPMENT.md)
- **Silo Panel** — SCADA-style operator UI embedded in the binary at `/silo`.
  [Guide](docs/platform/silo-panel.md)
- **Docs** — [developer portal](docs/developer/README.md) ·
  [production portal](docs/production/README.md) ·
  [security model](docs/SECURITY_MODEL.md) · [roadmap](ROADMAP.md)

---

## System Architecture

AdaTP runs on the **Tokio** async runtime with a message-passing design: every
connection is an independent task, and rooms route packets through
per-connection queues.

- **Transport**: a single **WebSocket** listener built on **axum 0.7 (`ws`
  feature)**, binary frames, default port `3000`, endpoint `/ws`. One AdaTP
  packet per WebSocket message. **The server terminates plain `ws://` — it has
  no TLS of its own; run it behind a TLS-terminating proxy** (see the security
  callout above). The pre-1.0 raw-TCP listener has been removed (`docs/legacy.md`).
- **State**: an in-memory concurrent connection/room registry built on
  **`DashMap`**. Nothing about rooms or messages is persisted; state is lost on
  restart and clients reconnect and re-join.
- **Routing**: room-scoped broadcast. **Plaintext** sessions are forwarded
  without per-recipient re-encryption; **encrypted** sessions (including
  voice/PCM) are **decrypted at the hub and re-encrypted per recipient** with
  each connection's own session keys. Delivery is **best-effort** — a slow
  consumer's full queue drops messages (counted in `dropped_messages`).
- **Authentication**: real credential verification on `AuthRequest` — `file`
  (`users.json`, dev only), `api` (external HTTP endpoint), or `none` (explicit
  anonymous mode). Unauthenticated connections cannot join rooms or send traffic.
- **Persistence**: `SQLite` (via SQLx) for HTTP API keys and webhook endpoints
  only.

---

## System Requirements

AdaTP is efficient — from a Raspberry Pi to a high-end server.

| Requirement | Minimum | Guidance for a busy single node |
| :--- | :--- | :--- |
| **OS** | Linux (any), macOS, Windows | Ubuntu 22.04 / Debian 11 |
| **CPU** | 1 core (Arm/x64) | 4+ cores (high frequency) |
| **RAM** | 512 MB | 8 GB+ |
| **Network** | 10 Mbps up/down | 1 Gbps+ (low jitter) |
| **Storage** | 100 MB free | NVMe SSD (for the DB + logs) |

> These are **capacity guidance, not benchmarked SLAs.** AdaTP's real cost is
> **fan-out** (room size × rate), not raw connection count, and it is
> single-node. Size from your traffic shape and verify on your hardware with
> the bundled harness — see [`docs/production/sizing.md`](docs/production/sizing.md)
> and the benchmark results in
> [`docs/production/benchmarks.md`](docs/production/benchmarks.md).

---

## Installation & Deployment

### One-line automated install (universal Linux)

Auto-detects the OS, installs dependencies (Rust, GCC, SSL), builds the server,
and sets up a `systemd` service (`adatp-server`).

```bash
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/setup.sh | bash
```

### Manual build (dev mode)

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo run --bin adatp-server
```

### Uninstall

```bash
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/uninstall.sh | bash
```

> **Before going live**, work through
> [`docs/production/checklist-go-live.md`](docs/production/checklist-go-live.md)
> and [`docs/production/security-hardening.md`](docs/production/security-hardening.md).
> TLS in front of the origin is a hard requirement.

---

## Authentication & Security

AdaTP verifies credentials on every connection (**fail-closed**) via one of
three drivers. The complete threat model — including exactly what the crypto
does and does not protect — is in
[`docs/SECURITY_MODEL.md`](docs/SECURITY_MODEL.md).

### 1. User file (default — development)
`AUTH_DRIVER=file` verifies against `users.json` (**plaintext demo
credentials — never production**). Reload at runtime via
`POST /admin/v1/users/reload`.

### 2. External API (production)
Delegate authentication to your backend (PHP / Node.js / Python / anything).

```env
AUTH_DRIVER=api
AUTH_API_URL=https://api.myapp.com/v1/verify_user
```

**Request (AdaTP → your API)**:
```json
POST /v1/verify_user
{ "username": "alice", "password": "user_provided_password" }
```

**Response (your API → AdaTP)**:
```json
// Success
{ "authorized": true, "user_id": "uuid-5566", "role": "admin" }
// Failure
{ "authorized": false, "error": "Invalid password" }
```

If the backend is unreachable, AdaTP answers `auth_unavailable` and **closes
the connection** — it never admits a client on backend failure.

### 3. Anonymous (`none`)
`AUTH_DRIVER=none` accepts every login with role `anonymous`. Explicit
open/dev mode only.

---

## Configuration (environment variables)

Set environment variables or a `.env` file in the root directory.

| Variable | Default | Description |
| :--- | :--- | :--- |
| `HOST` | `0.0.0.0` | Bind address. Use `127.0.0.1` to sit behind a local proxy. |
| `PORT` | `3000` | Listening port (WebSocket `/ws` + HTTP API on the same port). |
| `AUTH_DRIVER` | `file` | Credential verification: `file`, `api`, or `none` (anonymous dev mode). |
| `AUTH_FILE_PATH` | `users.json` | User file for the `file` driver (dev/demo only — plaintext passwords). |
| `AUTH_API_URL` | (none) | External verification endpoint for the `api` driver. Setting it selects `api` automatically. |
| `DATABASE_URL` | `sqlite:adatp.db` | SQLite database file (API keys + webhook endpoints). |
| `MAX_FRAME_BYTES` | `1048576` | Maximum accepted AdaTP payload size (bytes). |
| `IDLE_TIMEOUT_SECS` | `90` | Connections silent for longer are dropped (WS pings run every 30 s). |
| `PLUGINS_DIR` | `plugins` | Directory scanned for plugins at boot. |
| `ADMIN_TOKEN` | (generated) | Token for `/admin/v1` and the Silo Panel; a random one is logged if unset. **Set it explicitly in production.** |
| `MAX_CONNECTIONS` | `10000` | **Reporting hint only** (LB capacity %). The server does **not** enforce this cap. |
| `ADATP_WEBHOOK_ALLOW_PRIVATE` | `0` | Dev-only: allow webhook deliveries to private/loopback addresses (relaxes the SSRF guard). |
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace`. Use `warn` in production. |

Full reference: [`docs/production/configuration-reference.md`](docs/production/configuration-reference.md)

---

## Management CLI (Ubuntu / Debian / RHEL)

After installation, use these global commands:

```bash
adatp-status     # Active/Inactive
adatp-log        # live logs
adatp-restart    # apply config changes
adatp-stop       # stop
```

### Admin console
```bash
adatp                                            # connect to local server (anonymous)
adatp -u admin -p mypassword                     # with authentication
adatp --address 20.0.0.31:3000 -u alice -p secret  # remote, authenticated
```

---

## Client SDKs

The official SDKs are **independently versioned, separate repositories** (they
are not vendored in this repo — the links below are authoritative). Every SDK
speaks the same 45-byte frame; the C and Arduino/ESP32 SDKs additionally
implement the full on-device session handshake.

| Language | Repository | Notes |
| :--- | :--- | :--- |
| **C / Embedded** | [SDK-C](https://github.com/Ada-Transfer-Protocol/SDK-C) | The crown jewel — full X25519/AES-GCM on constrained MCUs |
| **Arduino / ESP32** | [SDK-ARDUINO-ESP32](https://github.com/Ada-Transfer-Protocol/SDK-ARDUINO-ESP32) | On-device peer for the Arduino/ESP32 ecosystem |
| **JavaScript / Web** | [SDK-JS](https://github.com/Ada-Transfer-Protocol/SDK-JS) | Browser: `AdaTPPhone`, `AdaTPChat`, `AdaTPConference` |
| **Node.js** | [SDK-NodeJS](https://github.com/Ada-Transfer-Protocol/SDK-NodeJS) | Server-side and tooling |
| **Python** | [SDK-Python](https://github.com/Ada-Transfer-Protocol/SDK-Python) | Scripting, agents, data pipelines |
| **PHP** | [SDK-PHP](https://github.com/Ada-Transfer-Protocol/SDK-PHP) | Web-backend integration |

---

## Project Structure

```
/Server
├── /server            # Server application (adatp-server, adatp-admin)
│   └── /silo          # Embedded operator UI assets
├── /core              # Protocol library (codec, crypto, sessions) + conformance tests
├── /plugins           # Bundled example plugins (echo, moderation)
├── /tools             # DevOps & utilities
│   ├── setup.sh / uninstall.sh / install_service.sh
│   ├── /adatp-cli     # Protocol test tool (WS handshake + login probe)
│   ├── /loadtest      # Load generator
│   ├── /webhook-receiver  # Signed-delivery dev receiver
│   └── /build         # Release tarball / buildx / doc-sync scripts
├── /docs              # Spec pack, developer & production portals, security model
│   ├── SPEC.md            # Normative index → /docs/spec/*
│   ├── SECURITY_MODEL.md  # Radically honest security model + threat model
│   └── PROTOCOL_SPEC.md   # One-page wire reference
├── /demos             # game-lobby (GameState tic-tac-toe)
├── ROADMAP.md         # Server-specific roadmap ladder
└── /tests             # Conformance vectors (+ multi-SDK suites in workspace)
```

## License
MIT License. Copyright (c) 2024 Ada Transfer Protocol Team.

[SDK-C]: https://github.com/Ada-Transfer-Protocol/SDK-C
[SDK-ARDUINO-ESP32]: https://github.com/Ada-Transfer-Protocol/SDK-ARDUINO-ESP32
