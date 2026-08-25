# AdaTP Specification — 04: Packets

**Status:** Normative, v1.0

This document is the **message-type registry** for protocol version 1 and
defines the payload of each type. Header layout is in
[03-framing.md](03-framing.md); connection-state admissibility is in
[05-state-machines.md](05-state-machines.md).

---

## 1. Registry

Legend — **Dir**: C→S client-to-server, S→C server-to-client.
**Routing**: *consumed* (server handles it), *routable* (broadcast to the
sender's room, sender included), *reply* (sent only to one connection).

| Code | Name | Dir | Routing | Status |
| :-- | :-- | :-- | :-- | :-- |
| `0x0001` | HandshakeInit | C→S | consumed | Active |
| `0x0002` | HandshakeResponse | S→C | reply | Active |
| `0x0003` | HandshakeComplete | C→S | consumed | Active |
| `0x0010` | AuthRequest | C→S | consumed | Active |
| `0x0011` | AuthChallenge | S→C | reply | **Reserved** |
| `0x0012` | AuthResponse | C→S | consumed | **Reserved** |
| `0x0013` | AuthSuccess | S→C | reply | Active |
| `0x0014` | AuthFailure | S→C | reply | Active |
| `0x0020` | TextMessage | C↔S | routable | Active |
| `0x0021` | TextAck | — | routable | **Reserved** |
| `0x0022` | TextRead | — | routable | **Reserved** |
| `0x0030` | FileInit | C↔S | routable | Active |
| `0x0031` | FileChunk | C↔S | routable | Active |
| `0x0032` | FileAck | C↔S | routable | Active (no server semantics) |
| `0x0033` | FileComplete | C↔S | routable | Active |
| `0x0034` | FileCancel | C↔S | routable | Active (no server semantics) |
| `0x0040` | VoiceInit | C↔S | routable | Active (signaling) |
| `0x0041` | VoiceOffer | C↔S | routable | Active (signaling) |
| `0x0042` | VoiceAnswer | C↔S | routable | Active (signaling) |
| `0x0043` | VoiceIce | C↔S | routable | Active (signaling) |
| `0x0044` | VoiceData | C↔S | routable | Active |
| `0x0045` | VoiceEnd | C↔S | routable | Active (signaling) |
| `0x0050` | **GameState** | C↔S | routable | Active |
| `0x0060` | PresenceUpdate | C↔S | routable (also server-emitted) | Active |
| `0x0061` | TypingIndicator | C↔S | routable | Active |
| `0x0070` | **ToolCall** | C→S | consumed | Active |
| `0x0071` | **ToolResult** | S→C | reply | Active |
| `0x0072` | **ToolError** | S→C | reply | Active |
| `0x0080` | Ping | C→S | consumed | Active *(relocated from 0x0070)* |
| `0x0081` | Pong | S→C | reply | Active *(relocated from 0x0071)* |
| `0x0090` | VideoInit | — | routable | **Reserved** *(relocated from 0x0050)* |
| `0x0091` | VideoOffer | — | routable | **Reserved** |
| `0x0092` | VideoAnswer | — | routable | **Reserved** |
| `0x0093` | VideoData | — | routable | **Reserved** *(relocated from 0x0053)* |
| `0x0094` | VideoEnd | — | routable | **Reserved** |
| `0x00A0` | JoinRoom | C→S | consumed | Active |
| `0x00A1` | RoomJoined | S→C | reply | Active |
| `0x00FF` | Disconnect | C↔S | consumed | Active |

> **Renumbering note (pre-1.0 history, informative).** Prior to v1.0,
> `0x0050–0x0054` were the video family and `0x0070/0x0071` were
> Ping/Pong. v1.0 assigns `0x0050` to GameState, `0x0070–0x0072` to the
> tool family, relocates Ping/Pong to `0x0080/0x0081`, and parks video at
> `0x0090–0x0094`. There are no deployed pre-1.0 protocol peers to
> interoperate with; implementations MUST use the v1 numbers above.

Reserved types MUST NOT be sent by v1 implementations. Receivers MUST
ignore (drop without error) any parseable packet whose type they do not
handle — including reserved and unknown codes
([09-extensions.md](09-extensions.md)).

## 2. Payload definitions

Payloads described as JSON are UTF-8 encoded JSON objects. On encrypted
sessions the JSON/binary payload is what gets encrypted; the schema below
describes the plaintext.

### 2.1 Handshake family

- **HandshakeInit (0x0001), C→S** — payload: the client's raw X25519
  public key, 32 bytes. A payload shorter than 32 bytes requests a
  **plaintext session**: the server acknowledges with an empty
  `HandshakeResponse` and no encryption is established.
- **HandshakeResponse (0x0002), S→C** — payload: the server's raw X25519
  public key (32 bytes), or empty for plaintext mode.
- **HandshakeComplete (0x0003), C→S** — the client's first `ENCRYPTED`
  packet, proving key agreement. Payload plaintext is conventionally the
  ASCII string `Verification OK`; the server verifies the GCM tag, not the
  text. On tag failure the connection closes
  (`handshake_verify_failed`).

### 2.2 Auth family

- **AuthRequest (0x0010), C→S** — JSON:
  `{"username": "<string>", "password": "<string>"}`. Extra members MUST
  be ignored by the server.
- **AuthSuccess (0x0013), S→C** — JSON:
  `{"user_id": "<string>", "username": "<string>", "role": "<string>"}`.
- **AuthFailure (0x0014), S→C** — JSON: `{"error": "<code>"}` where
  `<code>` is from [appendix-error-codes.md](appendix-error-codes.md).
  `AuthFailure` is also used to refuse pre-auth traffic
  (`not_authenticated`) and invalid room names (`invalid_room_name`).
- **AuthChallenge / AuthResponse (0x0011/0x0012)** — reserved for a future
  challenge–response scheme; not part of v1.

### 2.3 Text family

- **TextMessage (0x0020)** — UTF-8 text. Doubles as the signaling carrier;
  the grammar in [06-signaling.md](06-signaling.md) applies to payloads
  matching its prefixes. Maximum size is bounded only by
  `MAX_FRAME_BYTES`.
- **TextAck / TextRead (0x0021/0x0022)** — reserved delivery/read
  receipts. Not implemented in v1.

### 2.4 File family

File transfers are room-broadcast streams identified by a 16-byte
transfer id.

- **FileInit (0x0030)** — JSON:
  `{"id": "<uuid string>", "filename": "<string>", "size": <bytes>}`.
  Senders SHOULD include all three members.
- **FileChunk (0x0031)** — binary: `[transfer id (16 bytes)][chunk data]`.
  The transfer id bytes are the UUID from `FileInit` in binary form.
  Reference SDKs use 16 KiB chunks; chunks MUST fit `MAX_FRAME_BYTES`.
- **FileComplete (0x0033)** — binary: the 16-byte transfer id.
- **FileAck (0x0032) / FileCancel (0x0034)** — routable; receivers MAY use
  them for application-level flow control. The server assigns no
  semantics.

The server does not store files; delivery is live-stream only, at-most-once
per recipient ([02-architecture.md §6](02-architecture.md)).

### 2.5 Voice family

- **VoiceData (0x0044)** — one frame of raw PCM audio
  ([07-media-game.md](07-media-game.md)).
- **VoiceInit/Offer/Answer/Ice/End (0x0040–0x0043, 0x0045)** — routable
  signaling envelopes for applications that negotiate out-of-band media;
  the server assigns no semantics. v1 reference clients signal with the
  text grammar instead ([06-signaling.md](06-signaling.md)).

### 2.6 GameState (0x0050)

Opaque, room-routed shared state. The server treats the payload as bytes
and broadcasts it like `TextMessage` (sender included). JSON is
RECOMMENDED; the envelope
`{"v": 1, "game": "<id>", "state": {...}}` is RECOMMENDED for
interoperability but NOT enforced — see
[07-media-game.md §3](07-media-game.md).

### 2.7 Presence and typing

- **PresenceUpdate (0x0060)** — UTF-8: `JOIN`, `LEAVE`, or `BUSY`.
  Server-emitted on membership changes ([02-architecture.md §5](02-architecture.md));
  clients MAY also send it (`BUSY` etc.), and it routes normally.
- **TypingIndicator (0x0061)** — UTF-8, application-defined; routable.

### 2.8 Tool family

Defined normatively in [09-extensions.md](09-extensions.md). Summary:

- **ToolCall (0x0070), C→S** — JSON:
  `{"id": "<correlation id, client-chosen, ≤64 chars>", "tool": "<name>", "args": {...}}`
- **ToolResult (0x0071), S→C** — JSON:
  `{"id": "...", "tool": "...", "ok": true, "result": <any JSON>}`
- **ToolError (0x0072), S→C** — JSON:
  `{"id": "...", "tool": "...", "ok": false, "error": {"code": "<slug>", "message": "..."}}`

Tool packets are never broadcast: the reply goes only to the caller.

### 2.9 Liveness and teardown

- **Ping (0x0080), C→S** — arbitrary payload; server MUST reply `Pong`
  (0x0081) echoing the payload byte-for-byte. Allowed pre-auth.
- **Disconnect (0x00FF)** — optional UTF-8 reason. Client-sent: the server
  closes the connection (`client_disconnect`). Server-sent: emitted with
  reason `server_shutdown` during graceful shutdown/drain; the client
  SHOULD close upon receipt.

### 2.10 Rooms

- **JoinRoom (0x00A0), C→S** — UTF-8 room name; validity rules in
  [02-architecture.md §5](02-architecture.md).
- **RoomJoined (0x00A1), S→C** — UTF-8: the room name now joined.
