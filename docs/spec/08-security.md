# AdaTP Specification — 08: Security

**Status:** Normative, v1.0

This document specifies AdaTP's security mechanisms **exactly as
implemented**, states the threat model they address, and — deliberately —
the threats they do **not** address. The narrative companion is
`../protocol/crypto.md`.

---

## 1. Threat model

In scope:

- **T1** Passive network observers between client and server (when the
  AdaTP encryption layer or TLS is used).
- **T2** Unauthenticated peers attempting to use server resources: refused
  before `AuthSuccess`, bounded by the attempt/violation counters of
  [05-state-machines.md §3](05-state-machines.md).
- **T3** Cross-room information leakage: prevented by room-scoped routing.
- **T4** Resource exhaustion via oversized frames or slow consumers:
  bounded by `MAX_FRAME_BYTES`, the idle timeout, and drop-on-full
  outbound queues.

Explicitly **out of scope for the AdaTP layer** (mitigate with TLS and
deployment controls — §6):

- Active man-in-the-middle attackers (the key exchange is unauthenticated).
- A compromised or malicious server (there is no end-to-end encryption).
- Traffic analysis (packet sizes/timing are visible even when encrypted).
- Denial of service at the TCP/TLS layer.

## 2. Transport security layers

Two independent layers can protect a connection:

1. **TLS (`wss://`)** — terminated at a proxy/load balancer in front of
   the server. REQUIRED in production (§6).
2. **The AdaTP secure session** — the in-protocol X25519 → HKDF →
   AES-256-GCM channel below. OPTIONAL per connection; defense in depth
   over TLS, or best-effort confidentiality on trusted private networks.

A connection using neither is a **plaintext session** and MUST only be
used for development.

## 3. The AdaTP secure session

### 3.1 Key agreement

- The client generates an **ephemeral X25519** key pair and sends the raw
  32-byte public key in `HandshakeInit`.
- The server generates its own ephemeral X25519 pair and replies with the
  raw 32-byte public key in `HandshakeResponse`.
- Both compute the X25519 shared secret (32 bytes).
- Keys are per-connection and never reused; there are no long-term keys in
  v1 — which is also why the exchange is unauthenticated (§6.1).

### 3.2 Key derivation

From the shared secret, both sides derive four values with
**HKDF-SHA256** (extract-then-expand,
[RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)):

| Parameter | Value |
| :-- | :-- |
| salt (extract) | 32 zero bytes (`0x00` × 32) |
| IKM | the X25519 shared secret |
| info `"client_write"` → | 32-byte AES key for client→server packets |
| info `"server_write"` → | 32-byte AES key for server→client packets |
| info `"client_iv"` → | 12-byte IV root for client→server nonces |
| info `"server_iv"` → | 12-byte IV root for server→client nonces |

Golden vector `kdf-hkdf-sha256` in
[appendix-test-vectors.md](appendix-test-vectors.md) fixes the expected
outputs; every implementation MUST reproduce them.

### 3.3 Packet encryption

- Cipher: **AES-256-GCM**, no additional authenticated data (AAD is
  empty).
- Each direction keeps its own **sequence counter starting at 1**,
  incremented per encrypted packet sent; the value is carried in the
  header `sequence` field.
- Nonce: the sender's 12-byte IV root with its **last 8 bytes XORed with
  the little-endian sequence number**:

  ```
  nonce[0..4]  = iv_root[0..4]
  nonce[4..12] = iv_root[4..12] XOR le64(sequence)
  ```

- The 16-byte GCM tag follows the payload; `flags.ENCRYPTED` is set;
  `length` counts the ciphertext only.
- **Nonce reuse is fatal to GCM.** Senders MUST NOT reuse a sequence
  number under the same key; at 2⁶⁴ packets the connection MUST be closed
  and re-established (practically unreachable).

### 3.4 Handshake completion

The first encrypted client packet (`HandshakeComplete`, conventionally the
plaintext `Verification OK`) proves key agreement: the server verifies the
GCM tag and only then treats the session as secure. A tag failure closes
the connection (`handshake_verify_failed`). Once secure, the server
encrypts **all** its packets to that client.

### 3.5 Decryption failures and replay

- Any packet failing GCM authentication MUST cause connection close
  (`decrypt_failed`). There is no tolerance for tampered packets.
- Replay handling is **best-effort** in v1: receivers track the peer's
  highest sequence and derive nonces from the carried value; because the
  transport (WebSocket over TCP) is ordered and the nonce binds sequence
  to key, a replayed packet fails decryption or arrives out of order —
  but v1 does not maintain a strict anti-replay window and this is listed
  as a limitation (§6.4).

## 4. Authentication

- Credentials travel in `AuthRequest` — encrypted when a secure session is
  established, plaintext otherwise (hence §6.2: use TLS).
- The server verifies against exactly one configured driver:

| Driver | Verification | Intended use |
| :-- | :-- | :-- |
| `file` | username/password against a local JSON file (constant-time password comparison) | development, demos. The file stores **plaintext** passwords — never production. |
| `api` | `POST` to `AUTH_API_URL` with `{"username","password"}`; expects `{"authorized": bool, "user_id", "role"}` | production — delegate to your identity system |
| `none` | every login accepted, role `anonymous` | explicit open/dev mode only |

- **Fail closed:** if the driver errors (file unreadable at runtime, API
  unreachable), the server replies `AuthFailure {"error":"auth_unavailable"}`
  and closes. It MUST NOT admit on backend failure.
- Three failed attempts close the connection; pre-auth traffic is refused
  and bounded ([05-state-machines.md](05-state-machines.md)).

## 5. Control-plane security

- `/api/*` endpoints require an `x-api-key` header checked against the
  server's key store; keys are manageable via the bundled CLI.
- `/healthz` and `/readyz` are unauthenticated by design (probes).
- The admin plane `/admin/v1/*` is reserved; it MUST ship with its own
  authentication when specified.
- Secrets (`.env`, key stores, user files) MUST NOT be committed to
  version control.

## 6. Honest limitations (normative to acknowledge)

Implementations and deployments MUST NOT advertise properties beyond
these:

1. **No end-to-end encryption.** The server decrypts every packet to
   route it and re-encrypts per recipient. A compromised server reads all
   traffic. AdaTP's layer is client↔server transport security only.
2. **Unauthenticated key exchange.** Neither side proves possession of
   any identity during the X25519 handshake; an active MITM can sit in the
   middle of the AdaTP layer undetected. TLS (`wss://` with certificate
   verification) is the MITM defense and is therefore REQUIRED in
   production. (Ed25519 primitives exist in `adatp-core` but are not wired
   into the v1 handshake.)
3. **Credentials on plaintext sessions.** Without TLS and without a
   secure session, `AuthRequest` crosses the network in the clear. Servers
   SHOULD be deployed so this combination cannot occur (TLS everywhere).
4. **Best-effort replay protection** (§3.5) — no strict sliding window.
5. **The `file` driver is a development convenience** — plaintext
   passwords, no lockout beyond the per-connection counters, no rate
   limiting across reconnects in v1.
6. **No per-room authorization.** Any authenticated user may join any
   room name. Confidential rooms require unguessable names or an external
   authorization layer (via the `api` driver's role field and a gateway).

## 7. Production requirements checklist

A production deployment MUST:

- [ ] Terminate TLS (`wss://`) in front of the server; expose no plain
      `ws://` publicly ([../deployment/ports.md](../deployment/ports.md)).
- [ ] Use `AUTH_DRIVER=api` against a real identity backend (or a vetted
      replacement for the file driver).
- [ ] Keep `users.json`, `.env`, API keys and the SQLite store out of git
      and readable only by the service user.
- [ ] Rotate `/api` keys; delete the bootstrap key
      (`admin-secret-key`) created on first run.
- [ ] Leave `AUTH_DRIVER=none` disabled.
- [ ] Monitor `dropped_messages`, connection counts, and auth-failure
      logs.
