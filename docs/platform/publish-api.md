# HTTP publish API — `POST /publish`

Fan a message out to the members of one or more rooms over HTTP, without holding
a WebSocket. This is how an application server (e.g. a Laravel broadcast driver)
uses AdaTP as a broadcast transport.

The endpoint is **disabled** unless `ADATP_PUBLISH_SECRET` is set; without it
every request returns `503 publish_disabled`.

## Authentication

The same HMAC-SHA256 scheme as webhooks, over `<timestamp>.<raw-body>` so a
captured request cannot be replayed once its timestamp leaves the window.

| Header | Value |
| :-- | :-- |
| `x-adatp-timestamp` | Unix time in **milliseconds**. Rejected if more than 5 minutes from server time. |
| `x-adatp-signature` | `sha256=<hex>` where `<hex> = HMAC_SHA256(ADATP_PUBLISH_SECRET, "<timestamp>.<body>")`. Compared in constant time. |

```bash
TS=$(($(date +%s)*1000))
BODY='{"rooms":["lobby"],"event":"App\\Events\\Ping","payload":{"n":42}}'
SIG=$(printf '%s.%s' "$TS" "$BODY" | openssl dgst -sha256 -hmac "$ADATP_PUBLISH_SECRET" -hex | sed 's/^.* //')
curl -sS http://127.0.0.1:3000/publish \
  -H "content-type: application/json" \
  -H "x-adatp-timestamp: $TS" \
  -H "x-adatp-signature: sha256=$SIG" \
  --data "$BODY"
```

## Request body

One message object, or a JSON array of them (a batch — Laravel broadcasts to
several channels at once).

| Field | Type | Required | Meaning |
| :-- | :-- | :-- | :-- |
| `rooms` | string[] | yes | Rooms to deliver to. |
| `event` | string | yes | Event name (e.g. the Laravel event class). |
| `payload` | any (JSON) | no | Event data. |
| `exclude_session` | string (UUID) \| null | no | A client session id to skip — Laravel's `->toOthers()`. Honoured across the whole cluster. |

Each delivered message is a `TextMessage` whose payload is the JSON envelope
`{"event": <event>, "data": <payload>}`; every receiving connection re-encrypts
it under its own session keys, exactly as an in-band room broadcast does.

Limits: body ≤ 1 MiB; at most 100 room targets per request (summed across a
batch).

## Response

`200 OK`:

```json
{ "ok": true, "results": [ { "room": "lobby", "event": "App\\Events\\Ping", "delivered": 1 } ] }
```

`delivered` is the **local** node's delivery count. On a multi-node backplane,
deliveries on other nodes are not summed synchronously.

## Errors

Every error is `{ "ok": false, "error": "<code>", "message": "<detail>" }`.

| Status | `error` | When |
| :-- | :-- | :-- |
| 503 | `publish_disabled` | `ADATP_PUBLISH_SECRET` is not set. |
| 413 | `body_too_large` | Body exceeds 1 MiB. |
| 400 | `missing_timestamp` | No `x-adatp-timestamp` header. |
| 401 | `stale_timestamp` | Timestamp outside the 5-minute replay window. |
| 401 | `bad_signature` | HMAC mismatch. |
| 400 | `invalid_body` | Body is not valid JSON in the expected shape. |
| 400 | `too_many_rooms` | More than 100 room targets in the request. |

## Notes

- Delivery is best-effort (at-most-once), the same as an in-band broadcast: a
  slow consumer whose queue is full loses the message (counted in
  `dropped_messages`, see [observability](../production/observability.md)).
- The routed payload is plaintext at the hub (AdaTP is hop-by-hop, not E2E). Put
  the publish endpoint on a trusted network or behind TLS.
