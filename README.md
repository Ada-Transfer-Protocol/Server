# Ada Transfer Protocol (Server)

![AdaTP](https://img.shields.io/badge/AdaTP-v2.0-blueviolet?style=for-the-badge) ![Rust](https://img.shields.io/badge/Built%20With-Rust-orange?style=for-the-badge) ![License](https://img.shields.io/badge/License-MIT-green?style=for-the-badge) ![Uptime](https://img.shields.io/badge/Uptime-99.9%25-success?style=for-the-badge)

**AdaTP (Ada Transfer Protocol)** is a next-generation, high-performance real-time communication server built with Rust. It is designed to handle massive concurrency for Voice (VoIP), Signaling, and File Transfer with ultra-low latency.

Unlike traditional heavy protocols (SIP/WebRTC stacks), AdaTP uses a **lightweight binary framing protocol** over WebSocket/TCP, making it ideal for AI Agents, IoT devices, and High-Frequency Trading systems.

---

## 🏗 System Architecture

AdaTP is built on the **Tokio** asynchronous runtime, utilizing a **Message-Passing Actor Model** for state management.

*   **Networking Layer**: Uses `tokio-tungstenite` for WebSocket handling. Supports Binary frames directly (no Base64 overhead).
*   **State Management**: In-memory `ACID` compliant state maps protected by `RwLock` and `DashMap` for O(1) access times.
*   **Packet Routing**: Efficient broadcasting engine that routes audio packets (`0x0044`) without decoding/encoding (Zero-Copy forwarding).
*   **Persistence**: Uses `SQLite` (via SQLx) for user authentication and transaction logging.

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

## ⚙️ Configuration (Environment Variables)

You can configure the server by setting environment variables or creating a `.env` file in the root directory.

| Variable | Default | Description |
| :--- | :--- | :--- |
| `HOST` | `0.0.0.0` | Bind address. Use `127.0.0.1` for local only. |
| `PORT` | `3000` | Listening port for WebSocket connections. |
| `DATABASE_URL` | `sqlite:adatp.db` | Path to the SQLite database file. |
| `RUST_LOG` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace`. |

---

## 🛠 Management CLI

After installation via the script, use these global aliases to manage the server:

| Command | Description |
| :--- | :--- |
| `adatp-status` | Show service health (`systemctl status adatp-server`). |
| `adatp-log` | Tail live logs (`journalctl -u adatp-server -f`). |
| `adatp-restart` | Restart the process. |
| `adatp-stop` | Stop the service. |
| `adatp` | Launch the **Interactive Admin CLI**. |

---

## 📂 Project Structure

```
/adatp-server
├── /server           # Core Server Application
│   ├── main.rs       # Entry point & Tokio runtime setup
│   ├── api.rs        # WebSocket handler & Packet routing logic
│   └── state.rs      # In-memory Room & User state management
│
├── /core             # Shared Libraries (Used by Client & Server)
│   ├── lib.rs        # Protocol Definitions (Bitwise flags, Opcodes)
│   └── models.rs     # Structs for JSON payloads
│
├── /tools            # DevOps & Utilities
│   ├── setup.sh      # Universal Installer Script
│   ├── uninstall.sh  # Uninstaller
│   ├── install_service.sh # Systemd Generator
│   └── /adatp-cli    # Rust-based Admin CLI tool
│
├── /docs             # Documentation
│   └── PROTOCOL_SPEC.md # Binary Protocol Specification (RFC-style)
```

---

## 📦 Client SDKs

*   **JavaScript / Web**: [Ada-Transfer-Protocol/SDK-JS](https://github.com/Ada-Transfer-Protocol/SDK-JS)
    *   *Modules: Phone, Chat, Conference, File Transfer*
*   *(Coming Soon)*: Python SDK
*   *(Coming Soon)*: C++ Embedded SDK

## License
MIT License. Copyright © 2024 Ada Transfer Protocol Team.
