# Webhooks for Application Developers

Your backend can react to realtime events — logins, room joins, finished
file transfers, tool calls, custom plugin events — without holding an
AdaTP connection: the server POSTs **signed JSON** to your HTTPS endpoint.
This is the consumer's quickstart; the full contract lives in
[WEBHOOK_DEVELOPMENT.md](../platform/WEBHOOK_DEVELOPMENT.md).

## What you can subscribe to

| Event | Fires when | Data highlights |
| :-- | :-- | :-- |
| `auth.success` / `auth.failure` | a login is accepted / rejected | `username`, `remote`, (`role` / `reason`) |
| `room.joined` | a client enters a room | `room`, `username` |
| `connection.closed` | a client disconnects | `username`, `reason` |
| `file.completed` | a file transfer finishes | `room`, `sender` |
| `tool.called` | any tool call completes | `tool`, `plugin`, `caller`, `ok`, `latency_ms` |
| `server.started` / `server.stopping` | lifecycle | `addr` |
| `webhook.test` | you press TEST | — |
| `plugin.<name>.<event>` | a plugin emits a custom event | plugin-defined |

Filters are exact names, prefixes like `auth.*` / `plugin.echo.*`, or `*`.

## Registering an endpoint

Via the [Silo Panel](../platform/silo-panel.md) → WEBHOOKS, or the
[admin API](../platform/admin-api.md):

```bash
curl -X POST http://127.0.0.1:3000/admin/v1/webhooks \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"url":"https://api.example.com/hooks/adatp","events":["auth.*","room.joined"]}'
# → {"ok":true,"id":"…","secret":"<shown exactly once — store it now>"}
```

Then `POST /admin/v1/webhooks/<id>/test` fires a `webhook.test` at it.
Localhost URLs are blocked by the SSRF guard unless the server runs with
`ADATP_WEBHOOK_ALLOW_PRIVATE=1` (development only).

## What a delivery looks like

```
POST /hooks/adatp
content-type: application/json
x-adatp-event: room.joined
x-adatp-delivery: 5b0c…            ← unique per delivery attempt-group
x-adatp-signature: sha256=9f41…    ← HMAC-SHA256 of the raw body

{"id":"5b0c…","event":"room.joined","timestamp":1787660000000,
 "source":"server","data":{"room":"lobby","username":"user1"}}
```

## Verify the signature (always, before parsing)

```js
import { createHmac, timingSafeEqual } from 'crypto';

function verify(rawBody, signatureHeader, secret) {
    const expected = 'sha256=' + createHmac('sha256', secret).update(rawBody).digest('hex');
    return signatureHeader.length === expected.length &&
        timingSafeEqual(Buffer.from(signatureHeader), Buffer.from(expected));
}
// Express: use express.raw() for this route — the HMAC covers the RAW body.
```

Python equivalent (`hmac.new(secret, body, 'sha256')` +
`hmac.compare_digest`) and the checklist:
[receiver guide](../platform/WEBHOOK_DEVELOPMENT.md).

## Semantics you must design for

- **At-least-once**: failures retry (up to 5 attempts, exponential
  backoff) — deduplicate on `x-adatp-delivery` / body `id`.
- **Unordered**: don't infer sequence across events.
- **Answer fast** with 2xx; do real work async. 10 s timeout, redirects
  are not followed; 5 consecutive failures open a 60 s circuit breaker.
- Watch delivery health in Silo → WEBHOOKS (per-endpoint counters, audit
  of the last 256 deliveries).

## Local development loop

```bash
node tools/webhook-receiver/receiver.mjs 9099 my-secret   # prints + verifies
ADATP_WEBHOOK_ALLOW_PRIVATE=1 ADMIN_TOKEN=dev ./target/release/adatp-server
# register http://127.0.0.1:9099/hook with secret my-secret, press TEST
```
