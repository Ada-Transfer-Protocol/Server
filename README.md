# Ada Transfer Protocol (Server)

**High-Performance Real-Time Communication Server** powered by Rust.

This repository contains the reference implementation of the **AdaTP Server**, designed to handle massive concurrency for Voice, Video, and Signaling with minimal latency.

## 🚀 Features

*   **Ultra-Low Latency**: Built on `tokio` asynchronous runtime.
*   **Binary Protocol**: Custom framing for minimal overhead (Header + Payload).
*   **Room Management**: Dynamic room creation and isolation.
*   **Global & Private Signaling**: Direct routing of signaling messages (`INVITE`, `ACCEPT`).
*   **Production Ready**: Includes systemd service scripts and CLI tools.
*   **Cross-Platform**: Runs on Linux, macOS, and Windows.

---

## 🛠 Installation & Usage

### 1. Development Mode

```bash
# Clone Repository
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server

# Run Development Server
cargo run --bin adatp-server
```
Server listens on `0.0.0.0:3000` by default.

### 2. Production Deployment (Linux Service)

We provide an automated script to build, install, and run AdaTP as a systemd service.

```bash
chmod +x tools/install_service.sh
./tools/install_service.sh
```

**What this script does:**
*   Compiles `adatp-server` and `adatp-cli` in release mode.
*   Installs binaries to `/usr/local/bin`.
*   Creates and enables a `systemd` service (`adatp`).
*   Adds useful shell aliases for management.

### 3. Management Commands

Once installed via the script, you can use these shortcuts:

| Command | Description |
| :--- | :--- |
| `adatp-status` | Check server status. |
| `adatp-restart` | Restart the server service. |
| `adatp-stop` | Stop the server. |
| `adatp-log` | Watch live server logs (`journalctl -f`). |
| `adatp --help` | interactive CLI tool. |

---

## 📚 Protocol Specification

For full binary details, see [PROTOCOL_SPEC.md](docs/PROTOCOL_SPEC.md).

## 📦 Client SDKs

*   **JavaScript / Web**: [Ada-Transfer-Protocol/SDK-JS](https://github.com/Ada-Transfer-Protocol/SDK-JS)

---

## 📂 Project Structure

*   **/server**: The Rust server implementation.
*   **/core**: Shared libraries and models.
*   **/tools**: CLI utilities and deployment scripts.

## License
MIT
