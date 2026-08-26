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
- **The v1 X25519 handshake is unauthenticated** (no certificates). It does not
  stop an active man-in-the-middle by itself — v1 deployments MUST terminate TLS
  (`wss://`) in front of the server. **Protocol v2 adds an authenticated
  handshake** (Ed25519-signed transcript + key pinning, header-AAD, downgrade
  floor; see [`docs/spec/12-authenticated-handshake.md`](docs/spec/12-authenticated-handshake.md))
  that resists MITM without TLS — but v2 is not yet spoken by every SDK, so TLS
  stays the blanket recommendation.
- Replay protection is **enforced** (a sequence at or below the highest accepted
  is rejected; the window advances only after the tag verifies), not just
  TCP-ordered.
- The `file` auth driver stores **plaintext passwords** — development and
  demos only. Use `AUTH_DRIVER=api` in production.
- The bootstrap HTTP API key (`admin-secret-key`) is well-known — rotate it
  immediately (`adatp-admin auth create` + revoke the default).
- **Pre-1.0 git history contains an example `users.json`** with demo credentials
  (e.g. `admin:secret_password`). These were never production secrets. They are
  not exploitable on a correctly configured deployment: `users.json` is
  git-ignored, the `file` driver is **fail-closed** (it refuses to start without
  a real user file), and the shipped `users.example.json` uses obvious
  `CHANGE_ME` placeholders. The history was **not** rewritten because that demo
  data is non-sensitive and rewriting would break the signed `v1.0.0` tag and its
  release. Production MUST supply its own credentials (or use `AUTH_DRIVER=api`).
- No built-in DDoS protection beyond frame limits, rate limits, and idle
  timeouts; put the server behind an edge that absorbs floods.

## Hardening checklist

See [`docs/production/security-hardening.md`](docs/production/security-hardening.md)
for the operator checklist (TLS, token rotation, SSRF policy, plugin review,
OS hardening).
