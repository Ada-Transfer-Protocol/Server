# Ada Transfer Protocol (Server)

![AdaTP](https://img.shields.io/badge/AdaTP-v2.0-blueviolet?style=for-the-badge) ![Rust](https://img.shields.io/badge/Built%20With-Rust-orange?style=for-the-badge) ![License](https://img.shields.io/badge/License-MIT-green?style=for-the-badge)

**High-Performance Real-Time Communication Server** designed for massive concurrency, ultra-low latency voice/video, and instant signaling.

---

## 🚀 Quick Install (One-Line)

Install **AdaTP Server** and **CLI Tools** as a background service on Linux/macOS with a single command:

```bash
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/setup.sh | bash
```

**Installer Output:**
```text
   _       _       _____ ____  
  /_\   __| | __ _|_   _|  _ \ 
 //_\\ / _` |/ _` | | | | |_) |
/  _  \ (_| | (_| | | | |  __/ 
\_/ \_/\__,_|\__,_| |_| |_|    

Select Install Mode:
1) Full Installation (Server + CLI + Service)
2) Development Setup (Clone only)
> 1

📦 Building Server (Release)...
📦 Building CLI...
⚙️  Creating Systemd Service...
✅ Service 'adatp' is ACTIVE.
```

---

## 🛠 Manual Installation & Development

### Prerequisites
*   **Rust (Cargo)**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### 1. Run in Dev Mode
```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo run --bin adatp-server
```

**Expected Output:**
```text
INFO  adatp_server > 🚀 AdaTP Server v2.0 started on 0.0.0.0:3000
INFO  adatp_server > 💾 Database connected: adatp.db
INFO  adatp_server > 🔌 WebSocket listening...
```

---

## 💻 Management CLI

Once installed via the script, you get powerful shortcuts managed by `systemd`.

| Command | Action | Example Output |
| :--- | :--- | :--- |
| **`adatp-status`** | Check service health | `● adatp.service - AdaTP Server... Active: active (running)` |
| **`adatp-log`** | Live server logs | `Jun 24 10:00:00 server adatp[123]: [INFO] New connection: 192.168.1.5` |
| **`adatp-restart`** | Restart service | `Restarting adatp.service... Done.` |
| **`adatp-stop`** | Stop service | `Stopping adatp.service... Done.` |

### Admin CLI Tool (`adatp`)
The `adatp` command allows you to inspect the running server state.

```bash
adatp inspect --room lobby
```
**Output:**
```json
{
  "room_id": "lobby",
  "users": [
    { "id": "A1B2", "role": "admin", "audio": "active" },
    { "id": "C3D4", "role": "guest", "audio": "muted" }
  ]
}
```

---

## 📚 Protocol & SDKs

AdaTP is built to be modular.

*   📖 **Protocol Spec**: [Read the Binary Specification](docs/PROTOCOL_SPEC.md)
*   🌐 **JavaScript SDK**: [Ada-Transfer-Protocol/SDK-JS](https://github.com/Ada-Transfer-Protocol/SDK-JS)
    *   *Includes: Phone, Chat, Conference, File Transfer modules.*

---

## 📂 Architecture

```
/adatp
├── /server       # Main Rust Server (Tokio + Tungstenite)
├── /core         # Shared Logic (Packets, Auth, Database)
├── /tools
│   ├── /adatp-cli      # Admin CLI Tool logic
│   ├── setup.sh      # One-line installer
│   └── install_service.sh # Systemd generator
└── /docs         # Documentation
```

## License
MIT © Ada Transfer Protocol Team
