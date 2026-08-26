# AdaTP Security Model

**Status:** v1.0 · radically honest edition
**Scope:** the AdaTP server and `adatp-core` as shipped. This document
describes *what the system actually does*, not what we would like it to do.
Where a control is incomplete or absent, it says so and points at the code.

Companion documents:

- [`docs/spec/08-security.md`](spec/08-security.md) — the normative security
  specification (what every conformant implementation MUST do).
- [`docs/protocol/crypto.md`](protocol/crypto.md) — the narrative crypto walkthrough.
- [`docs/production/security-hardening.md`](production/security-hardening.md) — the operator checklist.
- Repository [`SECURITY.md`](../SECURITY.md) — vulnerability reporting + supported versions.

---

## 0. TL;DR — read this before you trust anything

- AdaTP's own encryption is **hop-by-hop transport encryption, not
  end-to-end.** The server **decrypts every packet to route it** and
  **re-encrypts it per recipient**. A compromised or curious server sees all
  plaintext. There is no cryptographic secrecy *from the server*.
- The application-layer key exchange is **not authenticated.** Nothing proves
  the server's (or a peer's) identity during the X25519 handshake, so an
  active man-in-the-middle can transparently sit in the middle of the AdaTP
  layer alone.
- Therefore **TLS termination at a reverse proxy (`wss://`) is REQUIRED in
  production.** TLS — not AdaTP's session crypto — is what actually provides
  server authentication and MITM protection today. AdaTP's session encryption
  is best understood as **defense-in-depth on top of TLS**, or as best-effort
  confidentiality on a **trusted** private network.
- The AEAD's **AAD is empty on v1** — the packet header is not bound into the GCM
  tag (harmless behind TLS, relevant only on an untrusted v1 transport).
  **Protocol v2 binds the full header as AAD**, closing this. **Anti-replay is
  enforced on both:** a sequence at or below the highest already-accepted is
  rejected and the packet dropped. See [§5](#5-packet-integrity-aad-and-anti-replay).

If you take away one sentence: **run AdaTP behind TLS, use `AUTH_DRIVER=api`,
and treat the app-layer crypto as a bonus, not the perimeter.**

---

## 1. The two security layers

Two independent layers can protect an AdaTP connection. They are not
alternatives to each other in production — you want the first, and may add the
second.

| Layer | Provided by | Authenticates the server? | Stops an active MITM? | Status |
| :-- | :-- | :-- | :-- | :-- |
| **TLS (`wss://`)** | A reverse proxy / load balancer **you** run (nginx, Caddy, Cloudflare, a cloud LB) | **Yes** (certificate verification) | **Yes** | **Required in production.** The server itself speaks plain `ws://`/`http://` — it contains no TLS code. |
| **AdaTP secure session** | `adatp-core` (X25519 → HKDF-SHA256 → AES-256-GCM) | **No** (unauthenticated key exchange) | **No, by itself** | Optional, per-connection. Defense-in-depth over TLS, or best-effort confidentiality on a trusted network. |

A connection using **neither** layer is a **plaintext session** and is for
development only. In that mode `AuthRequest` credentials cross the wire in the
clear.

> **Why TLS is mandatory, in one line:** AdaTP's handshake gives you an
> encrypted channel but no proof of *who is on the other end*. TLS gives you
> the proof. Without it, encryption is a lock with the key taped to the door.

---

## 2. What the server actually does with your bytes

```
   client A ──[ AES-256-GCM, session A keys ]──►  ┌────────────┐
   client B ──[ AES-256-GCM, session B keys ]──►  │  adatp     │
   client C ──[ plaintext (trusted net only) ]─►  │  server    │
                                                  │            │
                        the server DECRYPTS ──►   │  plaintext │  ◄── sees everything
                        to route by room          │  in memory │
                                                  │            │
   client A ◄─[ AES-256-GCM, session A keys ]──   │  RE-ENCRYPTS
   client B ◄─[ AES-256-GCM, session B keys ]──   │  per recipient
   client C ◄─[ plaintext ]───────────────────   └────────────┘
```

Concretely (`server/src/hub.rs`, `server/src/connection.rs`):

1. An inbound encrypted packet is decrypted with **that sender's** session
   keys into plaintext.
2. The plaintext payload is placed on each room member's outbound queue
   (`Hub::broadcast`; the routed payload is always plaintext internally).
3. Each recipient connection re-encodes the payload with **its own** session
   keys and sequence counter before sending (`encode_for_peer`). For a
   plaintext recipient it is forwarded as-is.

This is why any "zero-copy fan-out" claim applies **only to plaintext
sessions** (the payload `Bytes` is reference-counted and shared). Encrypted
sessions — including voice/PCM — pay a fresh AES-GCM encryption **per
recipient**. It is also, precisely, why there is **no end-to-end
confidentiality**: routing requires plaintext at the hub.

---

## 3. The cryptographic construction (as implemented)

All of the following is exercised by the golden vectors in
[`tests/conformance`](../tests) and replayed by the Rust, Node.js and Python
implementations, and by the C / Arduino-ESP32 SDKs.

### 3.1 Key agreement — `core/src/crypto/x25519.rs`
- Client generates an **ephemeral X25519** key pair, sends the raw 32-byte
  public key in `HandshakeInit`.
- Server generates its **own ephemeral** X25519 pair, replies with its raw
  32-byte public key in `HandshakeResponse` (`connection.rs`,
  `MessageType::HandshakeInit`).
- Both compute the 32-byte shared secret. **Keys are per-connection and never
  reused. There are no long-term identity keys in v1** — which is exactly why
  the exchange is unauthenticated (see [§4](#4-what-this-does-and-does-not-protect)).

### 3.2 Key derivation — `core/src/crypto/key_derivation.rs`
**HKDF-SHA256** (RFC 5869), salt = 32 zero bytes, IKM = the shared secret,
expanded into four labelled sub-keys:

| info label | output | used for |
| :-- | :-- | :-- |
| `client_write` | 32-byte AES key | client → server packets |
| `server_write` | 32-byte AES key | server → client packets |
| `client_iv` | 12-byte IV root | client → server nonces |
| `server_iv` | 12-byte IV root | server → client nonces |

### 3.3 Packet encryption — `core/src/crypto/aes_gcm.rs`, `core/src/session/secure_session.rs`
- Cipher: **AES-256-GCM**. The 16-byte tag is carried after the payload;
  `flags.ENCRYPTED` is set.
- Each direction has its own **sequence counter starting at 1**, carried in
  the header `sequence` field.
- **Nonce** = the sender's 12-byte IV root with its **last 8 bytes XORed with
  the little-endian sequence number**:
  ```
  nonce[0..4]  = iv_root[0..4]
  nonce[4..12] = iv_root[4..12] XOR le64(sequence)
  ```
- **Nonce reuse is fatal to GCM.** A sender must never reuse a sequence number
  under one key; the connection must be re-established long before 2⁶⁴ packets
  (practically unreachable).

### 3.4 Handshake completion
The first encrypted client packet (`HandshakeComplete`) proves key agreement:
the server verifies the GCM tag and only then marks the session secure. A tag
failure closes the connection (`handshake_verify_failed`). From that point the
server encrypts **all** packets to that client.

---

## 4. What this does — and does NOT — protect

### Protects (in scope)
- **Passive eavesdropping on the wire** — when TLS and/or the AdaTP session is
  in use. Payloads are AEAD-encrypted; a passive observer learns only sizes
  and timing.
- **Unauthenticated peers using server resources** — refused before
  `AuthSuccess`; pre-auth traffic is bounded (10-violation cap → connection
  close, `connection.rs::unauthorized`).
- **Cross-room leakage** — routing is strictly room-scoped
  (`Hub::broadcast`); a client only receives its current room's traffic.
- **Tampered ciphertext** — any GCM authentication failure closes the
  connection (`decrypt_failed`). There is no tolerance for corrupted packets.
- **Replayed packets** — a sequence at or below the highest already-accepted is
  rejected (`CryptoError::ReplayDetected`) and the connection closed; the
  high-water mark advances only after the tag verifies, so a forged sequence
  cannot wedge it ([§5.2](#52-anti-replay-is-enforced)).
- **Silent plaintext downgrade of an established session** — once a secure
  session exists (`conn.secure.is_some()`), the server **refuses plaintext** for
  sensitive message types (`AuthRequest`, `JoinRoom`, `ToolCall`, routable
  text/data); an attacker cannot strip encryption from a live session to slip a
  sensitive message through in the clear. (Pre-handshake, anonymous, and
  plaintext-only flows are unaffected; this stops *passive* downgrade, not an
  active MITM.)
- **Resource exhaustion via oversized frames / slow consumers / floods** —
  bounded by `MAX_FRAME_BYTES`, the idle timeout, drop-on-full outbound queues,
  a per-connection message-rate limit (`MSG_RATE_LIMIT`) and an enforced
  total-connection cap (`MAX_CONNECTIONS`); see
  [§8](#8-denial-of-service-posture-honest).

### Does NOT protect (out of scope for the AdaTP layer — mitigate with TLS + deployment)
- **A malicious or compromised server.** No end-to-end encryption; the server
  reads and re-encrypts everything ([§2](#2-what-the-server-actually-does-with-your-bytes)).
- **An active man-in-the-middle.** The key exchange is **unauthenticated**:
  neither side proves an identity, so an on-path attacker can complete two
  separate handshakes and relay. **TLS is the only MITM defense today** and is
  therefore required in production.
- **Credentials on a plaintext session.** Without TLS and without a secure
  session, `AuthRequest` is in the clear. Deploy so this cannot happen.
- **Header-field integrity** — the packet header is not bound into the AEAD tag
  (empty AAD); see [§5.1](#51-the-aeads-aad-is-empty). (Replay itself is now
  rejected — [§5.2](#52-anti-replay-is-enforced).)
- **Traffic analysis** — packet sizes and timing are visible even when encrypted.
- **Volumetric DoS at the TCP/TLS layer** — no built-in scrubbing; put the
  server behind an edge that absorbs floods ([§8](#8-denial-of-service-posture-honest)).

---

## 5. Packet integrity: AAD and anti-replay

Historically these were the two places where the docs described the intent and
the code did something weaker. Both are now addressed: **anti-replay** is
enforced (§5.2), and the **empty-AAD** gap is closed in **protocol v2** (§5.1).

### 5.1 AAD: empty in v1, the header is bound in v2
`SecureSession::encrypt`/`decrypt` pass an **empty AAD** to AES-GCM for a **v1**
session (`core/src/session/secure_session.rs`), so on v1 only the payload is
authenticated:
- The 45-byte header (message type, session id, flags, timestamp, length) is
  **not** bound into the GCM tag.
- The `sequence` field is *implicitly* bound, because it derives the nonce — a
  forged sequence yields the wrong nonce and fails decryption. (It is now also
  *explicitly* range-checked for replay; see §5.2.)
- So an active attacker on an **untrusted** transport could flip other header
  bits (e.g. `msg_type`, `session_id`) on an encrypted v1 packet without the tag
  catching it. This is a reason v1 requires TLS.

**v2 closes this:** a `SecureSession::new_v2` session binds the full 45-byte
header (`PacketHeader::header_bytes()`) as the AEAD AAD, so `msg_type`,
`session_id`, `flags`, `sequence` etc. are tamper-evident — any header change
fails the tag. Verified by the `v2_binds_header_as_aad_v1_does_not` test and
end-to-end against the Node SDK. v1 keeps empty AAD so its golden vectors are
unchanged.

### 5.2 Anti-replay is enforced
The normative spec calls replay handling "best-effort." The v1 build now
enforces it: `SecureSession::decrypt` (`core/src/session/secure_session.rs`)
tracks the peer's high-water sequence number and **returns
`CryptoError::ReplayDetected` for any sequence at or below the highest
already-accepted** value. The caller drops the packet and closes the connection
(`connection.rs`, `decrypt_in` → `decrypt_failed`).

- **DoS-safe advance.** The high-water mark advances **only after the AEAD tag
  verifies**, so a forged or arbitrarily high sequence number cannot ratchet the
  counter forward and lock out the legitimate peer.
- **Highest-seen, not a sliding window.** It rejects any sequence ≤ the highest
  accepted and **tolerates forward gaps** (a skipped sequence does not stall the
  stream); it does not re-admit out-of-order packets that fall below the
  high-water mark.
- **Effect.** Because the nonce is a deterministic function of the sequence, a
  captured packet already carries a used sequence; it is now rejected before its
  plaintext is released, on any transport. This closes the replay gap earlier
  drafts of this document described.

This is the *sequence*/replay control only. Header-field integrity is still
limited by the empty AAD (§5.1), and it does not by itself defeat an **active
MITM**, who can run a fresh handshake — that still requires TLS
([§4](#4-what-this-does-and-does-not-protect)).

**Operator takeaway:** on **v1** the empty AAD (§5.1) is an integrity gap and a
reason TLS is mandatory; it is not exploitable behind TLS. **v2 binds the header
as AAD** and (with a pinned key + `ADATP_MIN_PROTOCOL_VERSION=2`) removes that
gap without TLS. Anti-replay (§5.2) is a shipped control on both. Neither the v1
AAD gap nor replay lets a passive observer inject or replay on the recommended
(TLS) deployment.

---

## 6. Authentication

`server/src/auth.rs`, `server/src/connection.rs`.

- Credentials arrive in `AuthRequest` (JSON `{username, password}`) —
  encrypted when a secure session exists, plaintext otherwise (→ use TLS).
- Exactly one driver is configured:

| Driver | Verification | Intended use |
| :-- | :-- | :-- |
| `file` (default) | username/password against a local JSON file, **constant-time** password compare | **development / demos only** — the file stores **plaintext** passwords |
| `api` | `POST AUTH_API_URL {username,password}` → `{authorized, user_id, role}`, 5 s timeout | **production** — delegate to your identity system |
| `none` | every non-empty login accepted, role `anonymous` | explicit open/dev mode only |

- **Fail-closed.** If the driver errors (file unreadable at runtime, API
  unreachable), the server answers `AuthFailure {"error":"auth_unavailable"}`
  and **closes the connection**. It never admits a client on backend failure
  (`connection.rs`, `AuthError::Unavailable` → `Flow::Close`).
- **Attempt bounds.** Three failed attempts close the connection
  (`MAX_AUTH_ATTEMPTS = 3`). Pre-auth traffic is refused and bounded
  (`MAX_PREAUTH_VIOLATIONS = 10`).
- **No cross-reconnect lockout in v1.** The counters are per-connection; there
  is no account-level lockout or backoff across reconnects. Put brute-force
  protection at the edge or in your `api` backend.

---

## 7. Authorization: rooms and the control plane

- **Room join is gated on authentication.** `JoinRoom`, `ToolCall`, and every
  routable data packet require `conn.authed` to be set; otherwise the server
  replies `AuthFailure {"error":"not_authenticated"}` and counts a pre-auth
  violation (`connection.rs`).
- **Room-join authorization exists and is default-permissive.** An explicit
  `JoinRoom` passes two authorization gates before it is honored:
  1. a **plugin `join` veto hook** (mirrors the auth/tool-before hooks) — a
     plugin may deny the join on the data path; and
  2. a **built-in config policy** — `ROOM_ALLOWLIST` (CSV; when **non-empty**,
     joins are restricted to exactly those room names) and
     `ROOM_PROTECTED_PREFIX` + `ROOM_PROTECTED_ROLE` (any room whose name starts
     with the prefix requires the authenticated user to hold that role).

  With none of these configured the default is unchanged: any authenticated user
  may join any room name (1–128 chars, no control characters). Confidential rooms
  can now be gated in-process, or still by **unguessable names** or an
  **external authorization layer** (e.g. your `api` driver / gateway using the
  `role` field).
- **Initial room placement is not policy-gated.** A freshly authenticated user
  is auto-placed in the default `global` room *before* any `JoinRoom`; the policy
  above gates only explicit joins. If you use `ROOM_ALLOWLIST`, include the
  default lobby (`global`) in it, and note that a protected prefix does not cover
  the initial auto-placement.
- **Sender identity is pinned per connection.** The session id is captured
  from the first packet and cannot be switched mid-stream.
- **Control plane:** `/api/*` requires an `x-api-key` header checked against
  the SQLite key store (constant-time). `/admin/v1/*` and the `/silo` panel
  are gated by `ADMIN_TOKEN` (a random one is generated and logged if unset —
  dev behavior; set it explicitly in production). `/healthz` and `/readyz` are
  unauthenticated by design (probes). **Do not expose `/admin`, `/silo`, or
  `/api` on the public vhost.**
- **Bootstrap secrets to rotate:** the first-run API key (`admin-secret-key`)
  is well-known — create a real key and revoke it (`adatp-admin`).

---

## 8. Denial-of-service posture (honest)

AdaTP now enforces per-connection message-rate and server-wide connection
limits in addition to the resource-safety bounds. What it still does **not** do
is scrub **L3/L4 volumetric floods** — that belongs at the edge:

| Control | Where | What it bounds |
| :-- | :-- | :-- |
| `MAX_FRAME_BYTES` (1 MiB default) | per inbound frame | memory amplification per message |
| `MSG_RATE_LIMIT` (200 msg/s/connection default; `0` disables) | per connection | a **token bucket in the read path**; exceeding it closes the connection (`rate_limited`) |
| `MAX_CONNECTIONS` (10000 default) | server-wide | **enforced**: over the cap the WebSocket upgrade returns **HTTP 503**; the slot is released on close |
| `IDLE_TIMEOUT_SECS` (90 default) + 30 s ping | per connection | dead/stalled connections are reaped |
| Outbound queue = 256 msgs/connection | per connection | a slow consumer **loses** messages (counted in `dropped_messages`) instead of stalling the room |
| `MAX_AUTH_ATTEMPTS = 3`, `MAX_PREAUTH_VIOLATIONS = 10` | per connection | credential-guessing and pre-auth flooding on a single socket |
| Plugin tool-call rate limits | per plugin/tool | fixed one-minute window (`rate_limit_per_min`) |

**What still does NOT exist:** any **L3/L4 volumetric flood scrubbing**. The
`MSG_RATE_LIMIT` token bucket throttles an *established* connection's data plane
and `MAX_CONNECTIONS` bounds the total socket count, but neither absorbs a
packet/SYN flood or high-rate connection churn at the network layer. Put the
server behind an edge/CDN that absorbs floods, and size rooms per
[`sizing.md`](production/sizing.md).

---

## 9. Webhooks and plugins (the parts that are genuinely strong)

These are shipped, tested, and safe to be confident about.

**Webhooks** (`server/src/webhooks.rs`):
- Every delivery is **HMAC-SHA256 signed** (`x-adatp-signature: sha256=…`);
  consumers verify with the per-endpoint secret.
- **SSRF guard** on every endpoint URL and on every delivery: `http(s)` only,
  no credentials in the URL, redirects disabled, and the host must **not**
  resolve to loopback / private / link-local / unspecified space (IPv4 **and**
  IPv6). Overridable only by `ADATP_WEBHOOK_ALLOW_PRIVATE=1` for local dev,
  which logs a warning.
- Async queue (never on the data path), 5-attempt exponential-backoff retries,
  and a per-endpoint **circuit breaker** (opens for 60 s after 5 consecutive
  failures). A bounded audit log records every outcome.

**Plugins** (`server/src/plugins/`):
- Run as **separate OS child processes** speaking NDJSON over stdio — a plugin
  crash cannot take the server down (restart with backoff; `errored` after 5
  crashes).
- **Default-deny** manifest permissions, JSON-Schema argument validation,
  per-tool timeouts and rate limits, correlation ids.
- Veto hooks (auth/join/text/file/tool) run on the data path bounded by
  `hook_timeout_ms` (default 500 ms) with a configurable failure policy.

---

## 10. Threat model at a glance

| # | Threat | On a **trusted** network (no TLS) | Behind **TLS** (recommended) |
| :-- | :-- | :-- | :-- |
| Passive eavesdropper | Mitigated by AdaTP session crypto (if enabled) | Mitigated by TLS (+ AdaTP as defense-in-depth) |
| Active MITM | v1: **not mitigated** (unauthenticated). **v2: mitigated** — authenticated handshake + pinned key, require via `ADATP_MIN_PROTOCOL_VERSION=2` | Mitigated by TLS certificate verification |
| Malicious server | **Not mitigated** — no E2E | **Not mitigated** — no E2E (accept this, or add E2E above AdaTP) |
| Header tamper | v1: **not mitigated** (empty AAD). **v2: mitigated** — header bound as AEAD AAD | Mitigated by TLS integrity |
| Replay of captured packets | Mitigated — sequence ≤ high-water rejected (§5.2) | Mitigated (AdaTP replay check + TLS ordering) |
| Plaintext downgrade of a live session | Mitigated — plaintext refused for sensitive types once a session exists | Mitigated (same) |
| Unauthorized resource use | Mitigated — auth gate, attempt caps, rate/connection limits | Mitigated |
| Unauthorized room join | Config-gated when enabled (`ROOM_ALLOWLIST` / protected prefix); otherwise open by default | Same |
| Cross-room leakage | Mitigated — room-scoped routing | Mitigated |
| Data-plane message flood | Mitigated in-process — `MSG_RATE_LIMIT`, `MAX_CONNECTIONS` | Mitigated (same) + edge/CDN |
| Volumetric L3/L4 flood | **Not mitigated** in-process | Mitigate at the edge/CDN |

The single most important row is **Active MITM**. On **v1** it is the reason TLS
is not optional. **v2** now provides an in-protocol answer (a ProVerif-verified
authenticated handshake + header-AAD + a downgrade floor), so a v2 deployment
with a pinned key and `ADATP_MIN_PROTOCOL_VERSION=2` is MITM-resistant without
TLS — but that posture is earned per deployment (the client must speak v2), and
until every SDK does, TLS remains the blanket recommendation. Malicious-server
(no E2E) is unchanged by v2.

---

## 11. The path forward — two honest options

AdaTP's session crypto is real and useful, but its **unauthenticated
handshake** is the defining security limitation. There are two legitimate
directions, and they are **not mutually exclusive**:

### Option A — Authenticate the handshake (an AKE)
Add a long-term **Ed25519 server identity** and a **signed handshake
transcript** (server signs the X25519 exchange; clients pin/verify the key),
turning the unauthenticated DH into a proper **authenticated key exchange**.
Add strict anti-replay and bind the header as AAD at the same time. This makes
the AdaTP layer stand on its own — valuable for embedded/field deployments
where terminating TLS on a constrained device or a private industrial bus is
awkward.

> **This is already partly in the codebase.** `adatp-core` ships working
> Ed25519 primitives (`core/src/crypto/ed25519.rs`: `sign`, `verify`,
> `public_key_bytes`) — they are simply **not wired into the v1 handshake**.
> The remaining work is protocol design (transcript, key distribution/pinning)
> and wiring, not new cryptography. This is the **defining next security
> milestone.**

### Option B — Delegate confidentiality + authentication to TLS
Treat TLS as the sole security boundary: it already provides server
authentication, confidentiality, integrity, ordering, and replay protection.
In this framing AdaTP's app-layer crypto is **defense-in-depth** (or an
optional convenience on trusted private networks), and the empty-AAD / replay
gaps become non-issues because TLS covers them. This is the **correct posture
today** and requires no protocol change — only disciplined deployment.

### The decision — A, sequenced through verification

The review favours B, and B is correct *for a general-purpose server behind
TLS*. But AdaTP's reason to exist is the **embedded-first** case (see the
README): a constrained MCU that cannot run a TLS stack but can run
X25519/Ed25519/AES-GCM on-device. Under B, that peer has **no** security
without TLS — which guts the one axis the project owns. **Therefore the chosen
direction is A** (authenticate the handshake), so the same verified,
authenticated exchange runs on an MCU and in a browser.

A is chosen, **but not shipped blind.** Rolling a hand-rolled AKE across the
server and six SDKs and then announcing "MITM-resistant" would be exactly the
docs-ahead-of-code failure this whole review is about. So the sequence is:

1. **Specify** — done: [`spec/12-authenticated-handshake.md`](spec/12-authenticated-handshake.md)
   (SIGMA-style: Ed25519 signature over the full transcript, key pinning/TOFU,
   downgrade defense, mandatory encryption, header-as-AAD), as an **opt-in
   protocol v2**; v1 is untouched.
2. **Formally model** — **done and run** ([`spec/formal/`](spec/formal/),
   results in [`spec/formal/RESULTS.md`](spec/formal/RESULTS.md)): ProVerif
   confirms secrecy **and** injective agreement (no MITM) for v2, and
   reconstructs the MITM for v1 — the symbolic "prove it, don't claim it" half.
3. **Verify** — the mixed-version **downgrade query is now modeled and passes**
   ([`spec/formal/adatp_v2_downgrade.pv`](spec/formal/adatp_v2_downgrade.pv)).
   Still remaining: an expert review and — for a product making a crypto claim —
   an independent audit ([`ROADMAP.md`](../ROADMAP.md) tier 9). The symbolic
   model assumes perfect primitives and perfect pinning; it is necessary, not
   sufficient.
4. **Implement** — **server done**: `session/handshake_v2.rs` +
   `server/src/connection.rs` negotiate v2 on `version>=2` (persistent Ed25519
   identity, v1 untouched), with published, machine-checked conformance vectors.
   **Remaining**: a reference SDK client (incl. C, to prove the MCU claim and
   give the first end-to-end test), then the other SDKs.
5. **Only then** update the security claims — after a client speaks v2 and an
   audit.

**No SDK client negotiates v2 yet**, so end-to-end the guarantee is unproven and
**TLS remains mandatory**. The shipped v1 wire is unchanged; what changed is that
the server now has a real, verified v2 reference implementation behind version
negotiation, not just a design.

**Recommendation:** ship Option B now (TLS mandatory — the current guidance),
and pursue Option A as the roadmap item that lets a ~20 KB-RAM MCU be a
first-class, mutually-authenticated peer without a full TLS stack. See
[`ROADMAP.md`](../ROADMAP.md).

---

## 12. Production security requirements (the short checklist)

A production deployment MUST:

- [ ] **Terminate TLS (`wss://`)** at a proxy in front of the server; expose
      no plain `ws://` publicly. (The server has no TLS of its own.)
- [ ] Use **`AUTH_DRIVER=api`** against a real identity backend; delete the
      demo `users.json`; never run `AUTH_DRIVER=none` in production.
- [ ] Set **`ADMIN_TOKEN`** explicitly from a secret store; block `/admin`,
      `/silo`, `/api` from the public internet.
- [ ] **Rotate** the bootstrap API key (`admin-secret-key`); keep `.env`, user
      files, API keys and the SQLite store out of git and readable only by the
      service user.
- [ ] Keep **`ADATP_WEBHOOK_ALLOW_PRIVATE` unset**; verify HMAC signatures at
      every webhook consumer.
- [ ] Monitor `dropped_messages`, connection counts, and auth-failure logs.

The full operator walkthrough is
[`docs/production/security-hardening.md`](production/security-hardening.md).
