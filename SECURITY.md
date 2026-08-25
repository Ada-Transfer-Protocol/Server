# AdaTP Security Policy

## Reporting a vulnerability

Open a **private security advisory** on the relevant GitHub repository
(`Ada-Transfer-Protocol/Server` for the server and protocol, the SDK repo
for client-side issues). Do not open public issues for exploitable bugs.
You should receive an acknowledgement within 7 days.

## Supported versions

| Version | Supported |
| :-- | :-- |
| 1.0.x | ✅ |
| pre-1.0 (raw-TCP era) | ❌ — upgrade; see `docs/legacy.md` |

## What AdaTP v1 protects — and what it does not

The complete threat model lives in
[`docs/spec/08-security.md`](docs/spec/08-security.md). Summary:

**Provided**

- Real credential verification on every connection (`file`/`api` drivers,
  fail-closed when the backend is down), three-strike lockout, pre-auth
  traffic refusal.
- Optional session encryption: X25519 → HKDF-SHA256 → AES-256-GCM with
  per-direction sequence-derived nonces and integrity tags.
- Room isolation enforced server-side; sender identity pinned per
  connection.
- Webhook deliveries signed with HMAC-SHA256; SSRF guards on endpoint URLs.
- Plugins run as separate OS processes under default-deny permissions.
- Admin plane gated by a bearer token with constant-time comparison.

**Explicitly NOT provided in v1 (honest limitations)**

- **No end-to-end encryption.** Session crypto is client↔server; the server
  sees plaintext to route it.
- **The X25519 handshake is unauthenticated** (no certificates). It does not
  stop an active man-in-the-middle by itself — production deployments MUST
  terminate TLS (`wss://`) in front of the server.
- Replay protection is best-effort (TCP-ordered sequences), not a strict
  sliding window.
- The `file` auth driver stores **plaintext passwords** — development and
  demos only. Use `AUTH_DRIVER=api` in production.
- The bootstrap HTTP API key (`admin-secret-key`) is well-known — rotate it
  immediately (`adatp-admin auth create` + revoke the default).
- No built-in DDoS protection beyond frame limits, rate limits, and idle
  timeouts; put the server behind an edge that absorbs floods.

## Hardening checklist

See [`docs/production/security-hardening.md`](docs/production/security-hardening.md)
for the operator checklist (TLS, token rotation, SSRF policy, plugin review,
OS hardening).
