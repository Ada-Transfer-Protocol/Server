# AdaTP Specification — 01: Requirements

**Status:** Normative, v1.0

This document states the requirements protocol version 1 is designed to
satisfy. Later documents define the mechanisms; this one defines the bar
they must clear. Requirement identifiers (`F-*`, `N-*`) are stable and may
be referenced by tests and reviews.

---

## 1. Functional requirements

### F-1 Transport
- **F-1.1** The protocol MUST run over WebSocket (RFC 6455) binary
  messages, one packet per message.
- **F-1.2** A single server listener MUST carry the data plane (`/ws`) and
  the HTTP control endpoints on the same port (default 3000).
- **F-1.3** The protocol MUST NOT require any transport other than
  WebSocket. TLS termination MAY happen upstream (`wss://` at a proxy).

### F-2 Framing
- **F-2.1** Every packet MUST begin with the fixed 45-byte header of
  [03-framing.md](03-framing.md); all multi-byte integers little-endian.
- **F-2.2** Decoders MUST reject frames with a wrong magic value, a
  truncated header, or a payload shorter than the declared length.
- **F-2.3** Encoders MUST set `length` to the payload byte count
  (excluding the auth tag).

### F-3 Authentication
- **F-3.1** A connection MUST NOT be able to join rooms or send routable
  traffic before an `AuthSuccess`.
- **F-3.2** Credential verification MUST be real: the server MUST verify
  against a configured driver (`file`, `api`, or the explicitly-anonymous
  `none`), and MUST fail closed when the driver is unavailable.
- **F-3.3** Failed authentication MUST produce `AuthFailure` with a
  machine-readable error code; repeated failure (3 attempts) MUST close the
  connection.

### F-4 Rooms and routing
- **F-4.1** A connection is a member of exactly one room at a time;
  after authentication it MUST be placed in the default room `global`.
- **F-4.2** `JoinRoom` MUST be confirmed with `RoomJoined` carrying the
  room name.
- **F-4.3** Routable packets MUST be delivered to every current member of
  the sender's room **including the sender** (clients rely on their own
  echo, e.g. for RTT measurement).
- **F-4.4** Packets MUST NOT leak across rooms (room isolation).
- **F-4.5** Membership changes MUST be announced: `PresenceUpdate "LEAVE"`
  to the room a connection departs, `PresenceUpdate "JOIN"` to the room it
  enters (excluding the joiner itself).

### F-5 Confidentiality (optional layer)
- **F-5.1** A client MAY upgrade its connection to an encrypted session via
  the X25519 handshake; the server MUST support it.
- **F-5.2** The encryption layer MUST be exactly:
  HKDF-SHA256 key derivation (salt = 32 zero bytes) and AES-256-GCM with
  the sequence-derived nonce of [08-security.md](08-security.md).
- **F-5.3** A plaintext client and an encrypted client in the same room
  MUST interoperate: the server re-encodes per recipient.

### F-6 Features
- **F-6.1** Text messaging and the text-signaling grammar of
  [06-signaling.md](06-signaling.md).
- **F-6.2** Chunked file transfer (`FileInit`/`FileChunk`/`FileComplete`)
  with a 16-byte transfer id prefix on every chunk.
- **F-6.3** Voice as raw PCM frames (`VoiceData`) per
  [07-media-game.md](07-media-game.md).
- **F-6.4** First-class shared game state (`GameState`, 0x0050) routed like
  text.
- **F-6.5** Tool invocation (`ToolCall`/`ToolResult`/`ToolError`) routed to
  the server's plugin layer, replies to the caller only — plus a
  `TextMessage` fallback encoding for minimal clients
  ([09-extensions.md](09-extensions.md)).

### F-7 Liveness and shutdown
- **F-7.1** The server MUST ping idle connections (WebSocket protocol
  pings) and MUST drop connections silent longer than the configured idle
  timeout (default 90 s).
- **F-7.2** Graceful shutdown MUST notify clients with
  `Disconnect` (`server_shutdown`) before closing.
- **F-7.3** `Ping` (0x0080) MUST be answered with `Pong` (0x0081) echoing
  the payload, in every connection state.

### F-8 Observability
- **F-8.1** The server MUST expose liveness (`/healthz`) and readiness
  (`/readyz`) endpoints.
- **F-8.2** The server MUST expose metrics (connections, bytes, rooms,
  dropped messages) over an authenticated HTTP endpoint.

## 2. Non-functional requirements

- **N-1 Determinism.** Independent implementations MUST produce
  byte-identical frames for the golden-vector inputs
  ([appendix-test-vectors.md](appendix-test-vectors.md)).
- **N-2 Bounded resources.** The server MUST enforce a maximum payload
  size (default 1 MiB) and bounded per-connection outbound queues; overload
  sheds messages rather than memory
  (see `../architecture/reliability.md`).
- **N-3 Fail closed.** Ambiguity resolves to refusal: undecryptable
  packets, malformed frames, and unavailable auth backends terminate or
  refuse, never silently admit.
- **N-4 Honesty.** Documentation MUST NOT claim properties the
  implementation does not have. Known gaps are listed in
  [08-security.md §6](08-security.md) and
  `../architecture/reliability.md`.
- **N-5 Portability.** A conforming client requires only: a WebSocket
  client, and (for the Secure level) X25519, HKDF-SHA256, AES-256-GCM.
  Reference implementations exist for browsers, Node.js, Python, PHP, C,
  and ESP32.
- **N-6 Single-node scope.** v1 defines no server-to-server federation or
  clustered room state; rooms live in one server process
  (`../architecture/reliability.md`).

## 3. Explicit non-goals of v1 (informative)

- End-to-end encryption between clients.
- Message persistence, replay, or offline delivery.
- Delivery receipts (`TextAck`/`TextRead` are reserved, not implemented).
- Server-side media transcoding (audio is forwarded verbatim).
- Version negotiation (see [10-versioning.md](10-versioning.md)).
