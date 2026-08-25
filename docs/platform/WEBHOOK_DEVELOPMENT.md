# AdaTP Webhook Development Guide

This is the canonical guide for consuming AdaTP webhooks. Everything here
matches the implementation in `server/server/src/webhooks.rs` exactly.

Related: [plugin guide](PLUGIN_DEVELOPMENT.md) (custom events) ·
[admin API](admin-api.md) (CRUD) · [Silo panel](silo-panel.md) (UI) ·
[event extension rules](../spec/09-extensions.md)

---

## 1. Concept and architecture

Webhooks push server- and plugin-emitted events to your HTTP endpoints as
signed JSON POSTs. Delivery is **fully asynchronous**:

```
data plane ──emit──▶ event bus ──▶ dispatcher ──▶ bounded queue (1024)
                                                     │
                                     2 delivery workers, retries,
                                     per-endpoint circuit breaker
                                                     │
                                                     ▼
                                            POST https://your-app/…
```

Voice, chat and game traffic never wait on a webhook: a slow or dead
endpoint costs you deliveries (counted), never data-plane latency.

## 2. Configuring endpoints

All management goes through the [admin API](admin-api.md) — or the
Silo panel's **WEBHOOKS** tab, which uses the same endpoints.

Create:

```bash
curl -X POST http://127.0.0.1:3000/admin/v1/webhooks \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{
        "url": "https://app.example.com/hooks/adatp",
        "events": ["auth.*", "room.joined", "plugin.moderation.*"],
        "description": "ops pipeline"
      }'
# → { "ok": true, "id": "6f9c…", "secret": "b1946ac9…" }
```

- `events` empty/omitted defaults to `["*"]`.
- `secret` is optional; the server generates one when omitted. Either way
  **the secret is returned exactly once, at creation** — store it then. It
  is never shown again (listings redact it).
- The URL is validated against the SSRF rules (§6) before the endpoint is
  accepted.

Manage:

| Call | Effect |
| :-- | :-- |
| `GET /admin/v1/webhooks` | List endpoints + per-endpoint stats (secret redacted). |
| `PATCH /admin/v1/webhooks/:id` `{"active": false}` | Pause / resume. |
| `DELETE /admin/v1/webhooks/:id` | Remove permanently. |
| `POST /admin/v1/webhooks/:id/test` | Queue a signed `webhook.test` delivery (no retries). |
| `GET /admin/v1/webhooks/audit` | Last 256 delivery outcomes. |

## 3. Event catalog

Built-in events (`source: "server"`):

| Event | `data` | Fired when |
| :-- | :-- | :-- |
| `server.started` | `{addr}` | The server finished booting. |
| `server.stopping` | `{}` | Graceful shutdown began. |
| `auth.success` | `{username, role, remote}` | A login succeeded. |
| `auth.failure` | `{username, remote, reason}` | A login was refused: `reason: "invalid_credentials"` for wrong credentials, `"forbidden"` when a policy plugin vetoed an otherwise-valid login. |
| `room.joined` | `{room, username}` | A client entered a room via JoinRoom. |
| `connection.closed` | `{username, remote, reason}` | An authenticated connection ended (`reason` = close reason, e.g. `peer_closed`, `idle_timeout`, `server_shutdown`). |
| `file.completed` | `{room, sender:{username, role, session}}` | A FileComplete passed through a room. |
| `tool.called` | `{tool, plugin, caller, ok, latency_ms}` | A plugin tool call finished (success or failure; `system.list_tools` excluded). |
| `webhook.test` | `{message}` | You pressed TEST. |

Plugin events (`source: "<plugin name>"`):

| Event | `data` | Fired when |
| :-- | :-- | :-- |
| `plugin.<name>.<event>` | plugin-defined | A plugin with the `emit:events` permission called `emit_event`. E.g. the bundled echo plugin emits `plugin.echo.echoed {length}`. |

### Filters

Each endpoint subscribes with a list of filters, matched per event:

| Filter | Matches |
| :-- | :-- |
| `room.joined` | Exactly that event. |
| `auth.*` | Any event starting with `auth.` (prefix wildcard — the `.*` suffix form only). |
| `*` | Everything. |

## 4. The HTTP envelope

Every delivery is a `POST` with `content-type: application/json`:

```http
POST /hooks/adatp HTTP/1.1
content-type: application/json
x-adatp-event: room.joined
x-adatp-delivery: 5b1e6a3c-8f0e-4d2f-9c37-1a2b3c4d5e6f
x-adatp-signature: sha256=1f8a3b…64 hex chars…

{"id":"5b1e6a3c-…","event":"room.joined","timestamp":1787660000000,
 "source":"server","data":{"room":"lobby","username":"user1"}}
```

| Part | Meaning |
| :-- | :-- |
| body `id` / `X-AdaTP-Delivery` | Unique delivery id — identical on every retry of the same delivery; use it for idempotency. |
| body `event` / `X-AdaTP-Event` | Event name. |
| body `timestamp` | Emission time, Unix milliseconds. |
| body `source` | `"server"` or the emitting plugin's name. |
| `X-AdaTP-Signature` | `sha256=` + lowercase hex of **HMAC-SHA256(secret, raw request body)**. |

## 5. Verifying signatures

Always compute the HMAC over the **raw body bytes** (before any JSON
parsing) and compare in constant time.

**Node.js:**

```js
import { createServer } from 'http';
import { createHmac, timingSafeEqual } from 'crypto';

const SECRET = process.env.ADATP_WEBHOOK_SECRET;

createServer((req, res) => {
    let body = '';
    req.on('data', c => body += c);
    req.on('end', () => {
        const presented = req.headers['x-adatp-signature'] ?? '';
        const expected = 'sha256=' +
            createHmac('sha256', SECRET).update(body).digest('hex');
        let valid = false;
        try {
            valid = presented.length === expected.length &&
                timingSafeEqual(Buffer.from(presented), Buffer.from(expected));
        } catch { valid = false; }

        if (!valid) { res.writeHead(401); res.end(); return; }

        const delivery = JSON.parse(body);
        console.log(delivery.event, delivery.data);
        res.writeHead(200); res.end('{}');       // fast 2xx, work async
    });
}).listen(9099);
```

**Python:**

```python
import hashlib, hmac, json, os
from http.server import BaseHTTPRequestHandler, HTTPServer

SECRET = os.environ["ADATP_WEBHOOK_SECRET"].encode()

class Hook(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        presented = self.headers.get("x-adatp-signature", "")
        expected = "sha256=" + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
        if not hmac.compare_digest(presented, expected):
            self.send_response(401); self.end_headers(); return

        delivery = json.loads(body)
        print(delivery["event"], delivery["data"])
        self.send_response(200); self.end_headers(); self.wfile.write(b"{}")

HTTPServer(("", 9099), Hook).serve_forever()
```

## 6. Delivery semantics

| Property | Behavior |
| :-- | :-- |
| Guarantee | **At-least-once.** Duplicates are possible (retries); order is **not** guaranteed. |
| Success | Any 2xx response within the 10 s request timeout. |
| Retries | Up to **5 attempts** total. Backoff between attempts: 1 s, 5 s, 25 s, 125 s (capped at 300 s). TEST deliveries never retry. |
| Redirects | **Not followed.** A 3xx counts as failure. |
| Circuit breaker | Per endpoint: **5 consecutive failures** open the breaker for **60 s**; deliveries during that window are skipped and counted (`skipped_breaker`). A single success closes it. |
| Queue | Bounded at **1024** in-flight deliveries; overflow drops the delivery and increments a counter (`dropped_events`) — visible symptoms of a chronically slow receiver. |

### SSRF guards

Endpoint URLs are checked at creation **and again before every delivery**:

- Scheme must be `http` or `https`; no credentials in the URL.
- The host must not resolve to a non-public address:
  - IPv4: loopback, RFC1918 private, link-local 169.254/16, unspecified,
    broadcast.
  - IPv6: loopback, unspecified, unique-local `fc00::/7`, link-local
    `fe80::/10`.
- Violations are audited as `blocked_ssrf` and never sent.

Local development override: start the server with
`ADATP_WEBHOOK_ALLOW_PRIVATE=1` to deliver to `127.0.0.1` receivers (the
integration suite does this). **Never set it in production.**

## 7. The local receiver tool

A ready-made receiver for development lives at
`tools/webhook-receiver/receiver.mjs`:

```bash
node tools/webhook-receiver/receiver.mjs 9099 dev-secret
# AdaTP webhook receiver listening on http://127.0.0.1:9099
# Verifying signatures with secret: dev-secret
```

Register it against a dev server (started with
`ADATP_WEBHOOK_ALLOW_PRIVATE=1`):

```bash
curl -X POST http://127.0.0.1:3000/admin/v1/webhooks \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"url":"http://127.0.0.1:9099/hook","events":["*"],"secret":"dev-secret"}'
curl -X POST -H "x-admin-token: $ADMIN_TOKEN" \
  http://127.0.0.1:3000/admin/v1/webhooks/<id>/test
```

Every delivery is printed with its event, delivery id, signature verdict
(✅/❌) and pretty-printed body.

## 8. Receiver implementation checklist

- [ ] **Verify the signature before parsing** the body; reject with 401 on
      mismatch.
- [ ] **Respond 2xx fast** (< 10 s, ideally < 1 s); enqueue heavy work.
      Slow receivers trip the breaker and eventually drop deliveries.
- [ ] **Idempotency**: dedupe on `X-AdaTP-Delivery` — retries reuse it.
- [ ] **Tolerate duplicates and reordering** (at-least-once, unordered).
- [ ] **Don't log the secret** or reflect headers into logs verbatim.
- [ ] Use HTTPS in production (the server refuses private-network targets
      but does not require TLS — your endpoint should).
- [ ] Alert on your own 4xx/5xx rates; the server's audit log shows its
      side (§9).

## 9. Operations

- **Audit log** (`GET /admin/v1/webhooks/audit`, Silo → WEBHOOKS →
  DELIVERY AUDIT): the last **256** delivery outcomes —
  `delivered | retrying | failed | skipped_breaker | blocked_ssrf`, with
  HTTP status and attempt number.
- **Per-endpoint stats** (in `GET /admin/v1/webhooks`): `delivered`,
  `failed`, `skipped_breaker`, plus a live `breaker_open` flag — the red
  LED in Silo.
- **Endpoint refresh**: CRUD changes apply immediately; the endpoint list
  is also re-read from the database every 60 s.
- **Secret rotation**: secrets are immutable per endpoint. Rotate by
  creating a **new** endpoint with the same URL and a fresh secret,
  verifying deliveries (TEST + audit), then deleting the old endpoint.
  During the overlap your receiver should accept either secret.
