# Webhooks — Operations

Developer-facing reference (envelope, signatures, receiver code):
[`../platform/WEBHOOK_DEVELOPMENT.md`](../platform/WEBHOOK_DEVELOPMENT.md).
This page is the operator's view.

## Endpoint hygiene

- One endpoint per consuming system, each with its **own secret** and the
  **narrowest event filter** that works (`auth.*`, `room.joined`) — never a
  lazy `*` in production; it couples your consumer's availability to every
  event source, including chatty plugins.
- `description` is your ops label — put the owning team and a ticket/runbook
  reference in it.
- Review the list monthly: `GET /admin/v1/webhooks` — delete endpoints whose
  consumers are gone (delivery failures to dead systems burn retries and
  open breakers for no benefit).

## Secrets

- The signing secret is returned **exactly once** at creation
  (`POST /admin/v1/webhooks` response). Store it in your secret manager in
  that moment; the API never returns it again (listings redact it).
- Rotation procedure (no built-in rotate — it's create-verify-delete):
  1. Create a new endpoint with the same URL/filters (new secret).
  2. Deploy the consumer accepting **both** signatures.
  3. `POST /admin/v1/webhooks/<new>/test` → confirm `delivered` in audit.
  4. Delete the old endpoint; drop the old secret from the consumer.
- A consumer that starts answering 401 to valid deliveries has lost its
  secret — treat as an incident, rotate.

## Delivery semantics you are operating

| Property | Value |
| :-- | :-- |
| Guarantee | at-least-once (duplicates possible; consumers dedupe on `X-AdaTP-Delivery`) |
| Attempts | 5 (initial + 4 retries), backoff 1 s → 5 s → 25 s → 125 s (cap 300 s) |
| Timeout | 10 s per attempt; only 2xx counts as delivered; redirects are **not** followed |
| Ordering | not guaranteed (2 workers + retries) |
| Queue | 1024 in memory — overflow and events during downtime are **dropped and counted**, not persisted |
| Breaker | 5 consecutive failures → open 60 s (deliveries skipped + counted), then trial |

Consumer SLO worth agreeing with owning teams: **respond 2xx in < 1 s,
process async**. A consumer that does its work inline before answering
will hit the 10 s timeout under load and ride the breaker.

## Watching health

- Silo → WEBHOOKS: green LED = active, amber = paused, **red = breaker
  open**; columns OK/FAIL/BRK are `delivered`/`failed`/`skipped_breaker`.
- `GET /admin/v1/webhooks/audit` — last 256 outcomes with HTTP status and
  attempt number. `retrying` entries followed by `delivered` = transient
  blips; `failed` at attempt 5 = the consumer lost that event permanently.
- Alert candidates: any endpoint with `breaker_open=true` for > 5 min;
  `failed` growing while `delivered` is flat.

## Target-down runbook

1. Confirm in audit: consecutive `retrying`/`skipped_breaker` for one
   endpoint id.
2. **PAUSE the endpoint** (Silo or `PATCH /admin/v1/webhooks/:id
   {"active":false}`) — retries for a dead target are wasted work, and
   pausing keeps the audit log readable for other endpoints.
3. When the consumer recovers: resume, then `POST …/:id/test` and verify
   `delivered`.
4. Accept the loss window: events during the outage were not queued to disk
   (by design in v1). If the consumer needs a full picture it must
   reconcile from your source of truth, not from webhooks.

## SSRF policy

Deliveries are refused unless the URL is `http(s)`, has no embedded
credentials, and resolves to a **public** address. That is a security
control protecting your internal network from a compromised admin token.

- `ADATP_WEBHOOK_ALLOW_PRIVATE=1` disables the guard globally.
  **Never set it in production.** The server logs a warning at boot when
  it is active — treat that line in a prod log as a finding.
- Internal consumers should be reached via a public-DNS ingress you
  firewall, not by disabling the guard.
- DNS is resolved per delivery; a target whose DNS flips to a private
  address starts failing with `blocked_ssrf` in the audit — that's the
  guard doing its job (investigate the DNS change).

## Retry storms

A mass consumer outage (e.g. your event platform down) means every event ×
every endpoint enters retry. The queue caps at 1024 and drops beyond —
`dropped_events` counts it. During a long consumer outage: pause the
affected endpoints; resume when healthy. Don't raise rates elsewhere to
"catch up" — there is no replay in v1.
