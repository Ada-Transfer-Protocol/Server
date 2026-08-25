# AdaTP Specification — 11: Conformance

**Status:** Normative, v1.0

This document defines what "implements AdaTP v1" means, in three
cumulative levels, and how to verify a claim mechanically.

---

## 1. Conformance levels

### Level 1 — **Core** (plaintext client or server)

A Core **client** MUST:

- C-1 Connect over WebSocket and exchange exactly one packet per binary
  message ([03-framing.md §2](03-framing.md)).
- C-2 Encode/decode the 45-byte header per [03-framing.md](03-framing.md)
  and reproduce the plaintext golden vectors byte-for-byte.
- C-3 Authenticate with `AuthRequest` and handle both `AuthSuccess` and
  `AuthFailure`.
- C-4 Join rooms (`JoinRoom` → `RoomJoined`) and send/receive
  `TextMessage`.
- C-5 Tolerate multiplexed traffic while awaiting replies
  ([05-state-machines.md §5](05-state-machines.md)) and ignore unknown
  packet types.
- C-6 Handle `PresenceUpdate` and `Disconnect`; answer WebSocket protocol
  pings (standard stacks do).

A Core **server** MUST additionally implement the full state machine of
[05-state-machines.md](05-state-machines.md): real credential
verification (fail closed), pre-auth refusal with counters, room
isolation, sender-inclusive broadcast, presence announcements, limits
(`MAX_FRAME_BYTES`, idle timeout), `Ping`→`Pong`, and the close-reason
discipline of [appendix-error-codes.md](appendix-error-codes.md).

### Level 2 — **Secure** (Core + encryption)

Additionally:

- S-1 Perform the X25519 handshake
  (`HandshakeInit`/`HandshakeResponse`/`HandshakeComplete`).
- S-2 Derive keys with HKDF-SHA256 exactly per
  [08-security.md §3.2](08-security.md), reproducing the
  `kdf-hkdf-sha256` vector.
- S-3 Encrypt/decrypt AES-256-GCM packets with sequence-derived nonces,
  reproducing the encrypted golden vectors, and hard-fail on tag errors
  (`reject-tampered-tag`).
- S-4 (server) Interoperate mixed rooms: re-encode routable traffic per
  recipient.

### Level 3 — **Tools** (Core + tool invocation; Secure recommended)

Additionally:

- T-1 (client) Send `ToolCall` and correlate `ToolResult`/`ToolError` by
  `id` ([09-extensions.md §2](09-extensions.md)).
- T-2 (server) Dispatch tool calls to registered tools, reply only to the
  caller, enforce the `tool_*` error contract, and provide
  `system.list_tools`.
- T-3 (server) Support the `TOOL:`/`TOOLRESULT:` text fallback
  ([06-signaling.md §6](06-signaling.md)).

Reference implementations: the bundled server targets Core+Secure+Tools;
JS (browser) targets Core (+Tools via fallback); Node.js, Python, PHP, C,
and Arduino/ESP32 target Core+Secure (Node.js and Python also Tools).

## 2. Verification procedure

### 2.1 Golden vectors (static)

Machine-readable source of truth:
`tests/conformance/vectors/adatp-v1-vectors.json`
(reproduced in [appendix-test-vectors.md](appendix-test-vectors.md)).

For each case an implementation under test MUST:

- **frame-*** — encode the given inputs and compare hex output
  byte-for-byte; and decode the expected frame back to the same fields.
- **kdf-hkdf-sha256 / nonce-seq-xor** — derive and compare.
- **reject-*** — attempt to decode/decrypt and verify the attempt is
  refused (any error surface counts; silently succeeding fails the case).

Vectors are regenerated only by
`tests/conformance/generate_vectors.mjs`; regeneration MUST be
byte-stable ([10-versioning.md §5](10-versioning.md)).

### 2.2 Integration suite (live)

From the workspace root:

```bash
bash tests/integration/run.sh          # picks a free port automatically
PORT=3210 bash tests/integration/run.sh  # pin a port explicitly
```

The suite builds the server, starts it with the demo user file, and runs:

- `ws_text_roundtrip.mjs` — Core assertions: auth success/failure,
  pre-auth refusal, join confirmation, sender echo, roommate delivery,
  room isolation.
- `secure_roundtrip.mjs` — Secure assertions: handshake, encrypted auth,
  encrypted roundtrip between two independently-keyed clients, tamper
  (wrong password) refusal.

Exit code 0 with all assertions `ok` is the pass criterion.

### 2.3 Claiming conformance

A conforming release MUST state, per component:
level (Core/Secure/Tools), role (client/server), the vector-file version
it was verified against, and any documented deviations. Deviations from
MUST-level requirements disqualify the claim; SHOULD-level deviations
MUST be listed.

## 3. Out of scope

Performance targets, uptime, and operational hardening are not
conformance criteria; see `../architecture/reliability.md` for the
delivery-semantics contract and its non-claims.
