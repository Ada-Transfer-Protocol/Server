# Ada Transfer Protocol (AdaTP) — Wire Specification v1.0

> This document describes the protocol exactly as implemented by the server
> and the official SDKs (JS, Node.js, Python, PHP, C, Arduino/ESP32).
> The full normative specification pack lives in the workspace `docs/spec/`
> directory; this file is the compact single-page reference.

## 1. Transport

- AdaTP v1 runs over **WebSocket** (RFC 6455) using **binary frames**.
- Canonical endpoint: `ws://<host>:3000/ws` (production: `wss://` terminated
  at a load balancer / Cloudflare on 443, forwarded to 3000).
- **Exactly one AdaTP packet per WebSocket message.** Frames are not
  concatenated; fragmented WebSocket messages are reassembled by the
  transport before parsing.
- Text frames are ignored. WebSocket protocol-level Ping is answered by all
  clients (the server pings every 30 s and drops connections idle > 90 s).
- The pre-1.0 raw-TCP listener (:8444) is discontinued. See `docs/legacy.md`.

## 2. Framing — the 45-byte header

Every packet is a fixed 45-byte header followed by the payload and an
optional 16-byte AES-GCM authentication tag. All integers are
**little-endian**.

```
offset size field       notes
0      4    magic       0x41444154 ("ADAT" when read LE byte order T,A,D,A)
4      1    version     1
5      2    flags       bit0 ENCRYPTED, bit1 COMPRESSED, bit2 RELIABLE
7      4    length      payload length in bytes (excludes auth tag)
11     8    sequence    per-direction counter for encrypted packets, else 0
19     2    msg_type    see §3
21     8    timestamp   sender clock, milliseconds
29     16   session_id  client identity (random UUID chosen by the client)
45     N    payload
45+N   16   auth_tag    present iff flags.ENCRYPTED
```

The `session_id` identifies the sender: the server pins it to the first
packet received on a connection and stamps it into every packet routed to
other room members.

## 3. Message types

| Hex | Name | Payload |
| :-- | :-- | :-- |
| `0x0001` | HandshakeInit | Client X25519 public key (32 B) — or empty for plaintext mode |
| `0x0002` | HandshakeResponse | Server X25519 public key (32 B) — or empty for plaintext mode |
| `0x0003` | HandshakeComplete | Encrypted verification message |
| `0x0010` | AuthRequest | JSON `{ "username": "...", "password": "..." }` |
| `0x0013` | AuthSuccess | JSON `{ "user_id", "username", "role" }` |
| `0x0014` | AuthFailure | JSON `{ "error": "<code>" }` |
| `0x0020` | TextMessage | UTF-8 text (chat + text signaling) |
| `0x0021` | TextAck / `0x0022` TextRead | reserved acknowledgements |
| `0x0030` | FileInit | JSON `{ "id", "filename", "size" }` |
| `0x0031` | FileChunk | `[file id (16 B)][data]` |
| `0x0032` | FileAck, `0x0033` FileComplete, `0x0034` FileCancel | file control |
| `0x0040-0x0045` | VoiceInit/Offer/Answer/Ice/Data/End | voice signaling + PCM audio |
| `0x0050` | GameState | opaque room-routed state (JSON recommended) |
| `0x0060` | PresenceUpdate | `"JOIN"` / `"LEAVE"` / `"BUSY"` |
| `0x0061` | TypingIndicator | UTF-8 |
| `0x0070` | ToolCall / `0x0071` ToolResult / `0x0072` ToolError | plugin tool platform (JSON, correlation id) |
| `0x0080` | Ping / `0x0081` Pong | echo payload |
| `0x0090-0x0094` | VideoInit/Offer/Answer/Data/End | video (reserved; relocated pre-1.0) |
| `0x00A0` | JoinRoom | UTF-8 room name (1–128 chars, no control chars) |
| `0x00A1` | RoomJoined | UTF-8 room name (server confirmation) |
| `0x00FF` | Disconnect | optional reason |

## 4. Session flow

```
Client                                  Server
  |-- HandshakeInit (X25519 pub) --------->|      (optional, for encryption)
  |<-- HandshakeResponse (X25519 pub) -----|
  |-- HandshakeComplete (encrypted) ------>|
  |-- AuthRequest {username,password} ---->|      (encrypted if session secure)
  |<-- AuthSuccess {user_id,role} ---------|      or AuthFailure {error}
  |-- JoinRoom "lobby" ------------------->|
  |<-- RoomJoined "lobby" -----------------|
  |<== TextMessage / VoiceData / File ====>|      room-scoped broadcast
  |-- Disconnect ------------------------->|
```

- A client MAY skip the handshake entirely and speak plaintext (used by the
  browser SDK). The server then answers `AuthRequest` in plaintext.
- Until `AuthSuccess`, only handshake packets, `AuthRequest`, `Ping` and
  `Disconnect` are accepted. Anything else is answered with
  `AuthFailure {"error":"not_authenticated"}` and never routed.
- Three failed `AuthRequest`s close the connection. If the verification
  backend is unavailable the server fails **closed**.
- After authentication the connection is placed in room `global`; `JoinRoom`
  moves it. A connection is in exactly one room at a time.
- Room broadcasts include the sender (clients use their own echo, e.g. for
  RTT measurement via `SYS:PING` text messages).
- On join, the previous room receives `PresenceUpdate "LEAVE"` and the new
  room (excluding the joiner) `PresenceUpdate "JOIN"`. On disconnect the
  room receives `PresenceUpdate "LEAVE"`.

## 5. Encryption (transport security)

AdaTP sessions can be upgraded to an encrypted channel:

1. **Key agreement**: X25519 ECDH. Client sends its ephemeral public key in
   `HandshakeInit`; the server replies with its own ephemeral key.
2. **Key derivation**: HKDF-SHA256 with salt = 32 zero bytes, IKM = the
   shared secret, expanding four values: `client_write` (32 B),
   `server_write` (32 B), `client_iv` (12 B), `server_iv` (12 B).
3. **Packet encryption**: AES-256-GCM, no AAD. The 12-byte nonce is the IV
   root with its last 8 bytes XORed with the little-endian `sequence`
   number. Each direction increments its own sequence starting at 1.
4. The 16-byte GCM tag is carried after the payload; `flags.ENCRYPTED` set.

**Honest scope:** this is *client–server transport encryption* (comparable
to TLS in scope), not end-to-end encryption between clients. The server
decrypts every packet to route it and re-encrypts per recipient. The
handshake is unauthenticated (no certificates / signature verification), so
it does not protect against an active man-in-the-middle by itself — use
`wss://` (TLS) in production and treat AdaTP encryption as defense in
depth. See `docs/spec/08-security.md`.

## 6. Signaling standard (text-based)

`TextMessage` (0x0020) carries chat and lightweight signaling:

- Call control: `INVITE:<target>:<room>`, `RINGING:<room>`,
  `ACCEPT:<room>`, `REJECT:<room>`, `BUSY:<room>`, `BYE`
- Discovery: `DISCOVERY:WHO_IS_HERE`, `DISCOVERY:I_AM_HERE`,
  `DISCOVERY:I_AM_LEAVING`
- Mute state: `MUTE:ON` / `MUTE:OFF`
- Heartbeat/RTT: `SYS:PING` (measured via the sender's own echo)

## 7. Audio

Raw PCM: 16 000 Hz, signed 16-bit little-endian, mono. Typical frame:
2048 samples (4096 bytes) per `VoiceData` packet.

## 8. Authentication drivers (server)

| Driver | Behaviour |
| :-- | :-- |
| `file` | users.json (`[{username,password,role}]`) — dev/demo only, plaintext |
| `api` | `POST AUTH_API_URL {username,password}` → `{authorized,user_id,role}` |
| `none` | accepts any credentials, role `anonymous` — explicit dev mode |
