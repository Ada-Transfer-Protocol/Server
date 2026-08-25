# Protocol Guide (developer's view)

The practical tour of the wire. The byte-law lives in the normative spec —
[`docs/spec/03-framing.md`](../spec/03-framing.md) and
[`04-packets.md`](../spec/04-packets.md); this page is the working
knowledge you need while building clients.

## The 45-byte header

Little-endian throughout:

| Offset | Size | Field | Notes |
| --: | --: | :-- | :-- |
| 0 | 4 | `magic` | `0x41444154` ("ADAT") |
| 4 | 1 | `version` | `1` |
| 5 | 2 | `flags` | bit0 `ENCRYPTED`, bit1 `COMPRESSED` (reserved), bit2 `RELIABLE` (reserved) |
| 7 | 4 | `length` | payload bytes (auth tag excluded) |
| 11 | 8 | `sequence` | per-direction counter on encrypted packets, else 0 |
| 19 | 2 | `msg_type` | table below |
| 21 | 8 | `timestamp` | sender clock, ms |
| 29 | 16 | `session_id` | sender identity |
| 45 | N | payload | |
| 45+N | 16 | auth tag | only when `ENCRYPTED` |

A real frame — plaintext `TextMessage` "Hello, AdaTP!" (this is golden
vector `frame-plaintext-text`; timestamp `1700000000000`, session id
`000102…0e0f`):

```
54 41 44 41  01  00 00  0d 00 00 00   magic "ADAT" · v1 · flags 0 · len 13
00 00 00 00 00 00 00 00               sequence 0
20 00                                 msg_type 0x0020 TextMessage
00 c0 89 5e 8b 01 00 00               timestamp
00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f   session id
48 65 6c 6c 6f 2c 20 41 64 61 54 50 21            "Hello, AdaTP!"
```

## One packet per WebSocket message

AdaTP over WebSocket is **message-framed**: encode one packet, send it as
one binary WS message; parse each received binary message as exactly one
packet. Never concatenate packets in a message, never split one across
messages (WebSocket fragmentation below that level is the transport's
business and invisible to you). Text frames are ignored by the server.

## Message types — v1 registry

| Code | Type | Payload |
| :-- | :-- | :-- |
| `0x0001–0x0003` | HandshakeInit / Response / Complete | X25519 keys, encrypted verifier |
| `0x0010` | AuthRequest | `{"username","password"}` |
| `0x0013` / `0x0014` | AuthSuccess / AuthFailure | identity JSON / `{"error":code}` |
| `0x0020` | TextMessage | UTF-8 (chat + [signaling](voice.md)) |
| `0x0021`, `0x0022` | TextAck, TextRead | reserved |
| `0x0030–0x0034` | FileInit / Chunk / Ack / Complete / Cancel | [file transfer](file-transfer.md) |
| `0x0040–0x0045` | Voice family (`0x0044` VoiceData) | [voice](voice.md) |
| `0x0050` | **GameState** | opaque, JSON recommended — [game state](game-state.md) |
| `0x0060` / `0x0061` | PresenceUpdate / TypingIndicator | `"JOIN"/"LEAVE"/"BUSY"` / UTF-8 |
| `0x0070–0x0072` | **ToolCall / ToolResult / ToolError** | [tools](tools-and-plugins.md) |
| `0x0080` / `0x0081` | Ping / Pong | payload echoed |
| `0x0090–0x0094` | Video family | reserved (routed, no semantics) |
| `0x00A0` / `0x00A1` | JoinRoom / RoomJoined | UTF-8 room name |
| `0x00FF` | Disconnect | optional reason |

(Renumbered before 1.0 — if you carry pre-release code, see
[versioning](versioning.md).)

## Sequences and encryption

On a secure session each direction keeps its own counter starting at **1**;
the sender stamps it into `sequence` and derives the AES-GCM nonce from it
(IV root XOR sequence). Plaintext packets carry `sequence 0`. You never
manage nonces yourself — every SDK does this inside
`encrypt`/`decrypt`. The crypto recipe (X25519 → HKDF-SHA256 →
AES-256-GCM) is specified in [`docs/spec/08-security.md`](../spec/08-security.md)
and narrated in [`docs/protocol/crypto.md`](../protocol/crypto.md).

## Conversation shape

```
connect ws://host:3000/ws
  [optional] HandshakeInit → HandshakeResponse → HandshakeComplete
  AuthRequest → AuthSuccess (or AuthFailure ×3 → closed)
  JoinRoom "lobby" → RoomJoined "lobby"
  … room traffic, interleaved, includes your own echo …
  Disconnect
```

Because the stream is multiplexed, **replies interleave with broadcasts**:
after sending `JoinRoom` you may receive a roommate's `PresenceUpdate`
before your `RoomJoined`. Wait for *types*, not for "the next packet" —
the SDKs expose this (`readNextPacketOfType`, `readPacketOfType`, …).

## Limits you'll meet

| Limit | Default |
| :-- | :-- |
| Max payload (`MAX_FRAME_BYTES`) | 1 MiB |
| Idle timeout (server WS-pings every 30 s) | 90 s |
| Failed logins per connection | 3 |
| Per-connection outbound queue (drop-on-full) | 256 messages |

Delivery is **at-most-once**, unordered across senders, never persisted —
see [`docs/architecture/reliability.md`](../architecture/reliability.md).
