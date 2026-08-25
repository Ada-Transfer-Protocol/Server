# AdaTP Port Reference

## Canonical ports

| Port | Scope | What runs there |
| :-- | :-- | :-- |
| **3000** | local / container default | The single AdaTP listener: WebSocket data plane (`/ws`), health probes (`/healthz`, `/readyz`), HTTP API (`/api/*`), admin plane (`/admin/v1/*`). |
| **443** | production edge | TLS (`wss://`) terminated by a load balancer, reverse proxy or Cloudflare, forwarded to the server's `3000`. |

There is exactly **one** listening port. Data plane and control plane share
it; the paths differ.

## Changing the port

The server reads `PORT` (fallback: `SERVER_PORT`) and `HOST`
(fallback: `SERVER_HOST`):

```bash
HOST=0.0.0.0 PORT=3000 ./adatp-server
```

Every SDK accepts a host + port (defaulting to `3000`, path `/ws`) or a full
URL:

| SDK | Default | Custom |
| :-- | :-- | :-- |
| JS (browser) | — (URL required) | `new AdaTPChat("ws://127.0.0.1:3000/ws", …)` |
| Node.js | `new AdaTPClient(host, 3000)` | `new AdaTPClient("wss://example.com/ws")` |
| Python | `AdaTPClient('127.0.0.1', 3000)` | `AdaTPClient(url='wss://example.com/ws')` |
| PHP | `new Client('127.0.0.1', 3000)` | `new Client('wss://example.com/ws')` |
| C | `adatp_client_create("127.0.0.1", 3000)` | path fixed to `/ws` |
| Arduino/ESP32 | `client.connect(host, 3000, user, pass)` | optional 5th arg = path |

## Retired ports

| Port | Status |
| :-- | :-- |
| 8444 (raw TCP data plane) | **Removed in v1.0** — see [`docs/legacy.md`](../legacy.md). |
| 8443 (old dev default) | Never a v1 port; any remaining references are bugs — report them. |

## Local development note

If something else (e.g. a Vite dev server) already occupies `3000`, run the
server on another port: `PORT=3100 ./adatp-server` — and point clients at it
explicitly. The integration suite honours the same variable
(`PORT=3100 bash tests/integration/run.sh`).
