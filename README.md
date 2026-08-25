# Ada Transfer Protocol (Server)

![AdaTP](https://img.shields.io/badge/AdaTP-v1.0.0-blueviolet?style=for-the-badge) ![Rust](https://img.shields.io/badge/Built%20With-Rust-orange?style=for-the-badge) ![License](https://img.shields.io/badge/License-MIT-green?style=for-the-badge)

**AdaTP (Ada Transfer Protocol)** is a high-performance realtime communication server built with Rust: chat, voice (raw PCM), file transfer, shared game state, an extensible plugin/tool platform, signed webhooks and a built-in operator panel — all over one WebSocket port.

Unlike heavyweight stacks (SIP/WebRTC), AdaTP uses a **lightweight 45-byte binary frame** over WebSocket, making it ideal for AI agents, IoT devices and low-latency applications. The protocol is fully specified ([docs/SPEC.md](docs/SPEC.md) → [docs/spec/](docs/spec/)) with golden test vectors replayed by three independent implementations.

**Platform highlights**

*   🔌 **Plugins & tools** — process-isolated plugins (any language) expose callable tools and policy hooks. [Guide](docs/platform/PLUGIN_DEVELOPMENT.md)
*   📡 **Webhooks** — HMAC-signed event deliveries with retries, circuit breaker and SSRF guards. [Guide](docs/platform/WEBHOOK_DEVELOPMENT.md)
*   🤖 **AI agents** — raw 16 kHz PCM, tool calling and GameState make agent integration first-class. [Guide](docs/developer/AI_AGENT_DEVELOPMENT.md)
*   🏭 **Silo Panel** — SCADA-style operator UI embedded in the binary at `/silo`. [Guide](docs/platform/silo-panel.md)
*   📚 **Docs** — [developer portal](docs/developer/README.md) · [production portal](docs/production/README.md) · [testing](docs/testing/README.md)

---

## 🏗 System Architecture

AdaTP is built on the **Tokio** asynchronous runtime with a message-passing design: every connection is an independent task, and rooms route packets through per-connection queues.

*   **Transport**: A single **WebSocket** listener (axum, binary frames) on port `3000`, endpoint `/ws`. One AdaTP packet per WebSocket message. The pre-1.0 raw-TCP listener has been removed (see `docs/legacy.md`).
*   **State Management**: In-memory connection/room registry built on `DashMap` for concurrent access.
*   **Packet Routing**: Room-scoped broadcast. Plaintext sessions get zero-copy forwarding; encrypted sessions are re-encrypted per recipient with that connection's session keys.
*   **Authentication**: Real credential verification on `AuthRequest` — `file` (users.json), `api` (external HTTP endpoint) or `none` (explicit anonymous mode). Unauthenticated connections cannot join rooms or send traffic.
*   **Persistence**: `SQLite` (via SQLx) for HTTP API keys.

---

## 💻 System Requirements

AdaTP is extremely efficient. It can run on a Raspberry Pi or a high-end server.

| Requirement | Minimum | Recommended (10k+ Users) |
| :--- | :--- | :--- |
| **OS** | Linux (Any), macOS, Windows | Ubuntu 22.04 / Debian 11 |
| **CPU** | 1 Core (Arm/x64) | 4+ Cores (High Frequency) |
| **RAM** | 512 MB | 8 GB+ |
| **Network** | 10 Mbps Up/Down | 1 Gbps+ (Low Jitter) |
| **Storage** | 100 MB free space | NVMe SSD (for DB logs) |

---

## 🚀 Installation & Deployment

### One-Line Automated Install (Universal Linux)

This script auto-detects your OS, installs dependencies (Rust, GCC, SSL), builds the server, and sets up a systemd service (`adatp-server`).

```bash
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/setup.sh | bash
```

### Manual Build (Dev Mode)

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo run --bin adatp-server
```

### Uninstall
To completely remove AdaTP from your system:
```bash
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/uninstall.sh | bash
```

---

## 🔐 Authentication & Security

AdaTP verifies credentials on every connection (fail-closed) with three drivers:

### 1. User file (default — development)
`AUTH_DRIVER=file` verifies against `users.json` (plaintext demo credentials
— never production). Reload at runtime via `POST /admin/v1/users/reload`.

### 2. External API (production)
You can delegate authentication to your custom backend (e.g. PHP/Node.js/Python).

Add this to your `.env` file:
```env
AUTH_DRIVER=api
AUTH_API_URL=https://api.myapp.com/v1/verify_user
```

**Request (AdaTP -> Your API)**:
```json
POST /v1/verify_user
{
  "username": "alice",
  "password": "user_provided_password"
}
```

**Response (Your API -> AdaTP)**:
```json
// Success
{
  "authorized": true,
  "user_id": "uuid-5566",   // Used as PeerID
  "role": "admin"           // Optional
}

// Failure
{
  "authorized": false,
  "error": "Invalid password"
}
```

---

## ⚙️ Configuration (Environment Variables)

You can configure the server by setting environment variables or creating a `.env` file in the root directory.

| Variable | Default | Description |
| :--- | :--- | :--- |
| `HOST` | `0.0.0.0` | Bind address. Use `127.0.0.1` for local only. |
| `PORT` | `3000` | Listening port (WebSocket `/ws` + HTTP API on the same port). |
| `AUTH_DRIVER` | `file` | Credential verification: `file`, `api`, or `none` (anonymous dev mode). |
| `AUTH_FILE_PATH` | `users.json` | User file for the `file` driver (dev/demo only — plaintext passwords). |
| `AUTH_API_URL` | (none) | External verification endpoint for the `api` driver. Setting it selects `api` automatically. |
| `DATABASE_URL` | `sqlite:adatp.db` | Path to the SQLite database file (API keys). |
| `MAX_FRAME_BYTES` | `1048576` | Maximum accepted AdaTP payload size (bytes). |
| `IDLE_TIMEOUT_SECS` | `90` | Connections silent for longer are dropped (WS pings run every 30s). |
| `PLUGINS_DIR` | `plugins` | Directory scanned for plugins at boot. |
| `ADMIN_TOKEN` | (generated) | Token for `/admin/v1` and the Silo Panel; a random one is logged if unset. |
| `ADATP_WEBHOOK_ALLOW_PRIVATE` | `0` | Dev-only: allow webhook deliveries to private/loopback addresses. |
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace`. |

Full reference: [docs/production/configuration-reference.md](docs/production/configuration-reference.md)

---

## 🛠 Management CLI (Ubuntu / Debian / RHEL)

After installation, use these global commands to manage the server:

### Service Control
```bash
# Check Server Status (Active/Inactive)
adatp-status

# View Live Logs (Real-time)
adatp-log

# Restart Server (Apply config changes)
adatp-restart

# Stop Server
adatp-stop
```

### Admin Console
Launch the interactive command-line interface to inspect the server:
```bash
# Connect to local server (Anonymous)
adatp

# Connect with Authentication
adatp -u admin -p mypassword

# Connect to remote server with credentials
adatp --address 20.0.0.31:3000 --username alice --password secret
```

---

## 📚 Client SDKs

Integrate AdaTP into your applications using our official SDKs.

| Language | Repository | Status |
| :--- | :--- | :--- |
| **JavaScript / Web** | [SDK-JS](https://github.com/Ada-Transfer-Protocol/SDK-JS) | ✅ Stable |
| **Node.js** | [SDK-NodeJS](https://github.com/Ada-Transfer-Protocol/SDK-NodeJS) | ✅ Stable |
| **Python** | [SDK-Python](https://github.com/Ada-Transfer-Protocol/SDK-Python) | ✅ Stable |
| **PHP** | [SDK-PHP](https://github.com/Ada-Transfer-Protocol/SDK-PHP) | ✅ Stable |
| **C / Embedded** | [SDK-C](https://github.com/Ada-Transfer-Protocol/SDK-C) | ✅ Stable |
| **Arduino / ESP32** | [SDK-ARDUINO-ESP32](https://github.com/Ada-Transfer-Protocol/SDK-ARDUINO-ESP32) | ✅ Stable |

### JavaScript SDK Features
The Web SDK supports `AdaTPPhone` (VoIP), `AdaTPChat` (Messaging), and `AdaTPConference` with a low-code config pattern.

---

## 📂 Project Structure

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
├── /docs              # Spec pack, developer & production portals, guides
│   ├── SPEC.md        # Normative index → /docs/spec/*
│   └── PROTOCOL_SPEC.md  # One-page wire reference
├── /demos             # game-lobby (GameState tic-tac-toe)
└── /tests             # Conformance vectors (+ multi-SDK suites in workspace)
```

## License
MIT License. Copyright © 2024 Ada Transfer Protocol Team.
