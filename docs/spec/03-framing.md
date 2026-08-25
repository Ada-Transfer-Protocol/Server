# AdaTP Specification — 03: Framing

**Status:** Normative, v1.0

This document defines the byte-exact wire format. A conforming encoder and
decoder can be built from this document alone and MUST reproduce the golden
vectors in [appendix-test-vectors.md](appendix-test-vectors.md).

---

## 1. Packet layout

Every AdaTP packet is:

```
[ header : 45 bytes ][ payload : length bytes ][ auth_tag : 16 bytes, iff ENCRYPTED ]
```

All multi-byte integers are **little-endian**. There is no padding and no
alignment.

### 1.1 Header fields

| Offset | Size | Field | Type | Rules |
| --: | --: | :-- | :-- | :-- |
| 0 | 4 | `magic` | u32 LE | MUST be `0x41444154`. On the wire this is the bytes `54 41 44 41` (`"TADA"` read left-to-right; the constant spells `"ADAT"`). Receivers MUST reject any other value (`invalid_magic`). |
| 4 | 1 | `version` | u8 | MUST be `1` for this specification. See [10-versioning.md](10-versioning.md). |
| 5 | 2 | `flags` | u16 LE | Bit field, §1.2. Unknown bits MUST be ignored on receipt and SHOULD be zero on send. |
| 7 | 4 | `length` | u32 LE | Payload byte count, **excluding** the auth tag. |
| 11 | 8 | `sequence` | u64 LE | For `ENCRYPTED` packets: the sender's per-direction counter used in nonce derivation, starting at 1. For plaintext packets: SHOULD be 0 and MUST be ignored. |
| 19 | 2 | `msg_type` | u16 LE | Registry in [04-packets.md](04-packets.md). |
| 21 | 8 | `timestamp` | u64 LE | Sender clock in milliseconds. Informational; receivers MUST NOT use it for security decisions. |
| 29 | 16 | `session_id` | 16 bytes | Sender identity label ([02-architecture.md §4](02-architecture.md)). Raw bytes, no endianness. |
| 45 | N | `payload` | bytes | `length` bytes. For `ENCRYPTED` packets this is ciphertext. |
| 45+N | 16 | `auth_tag` | bytes | Present **iff** `flags.ENCRYPTED`. AES-256-GCM tag over the payload. |

Total minimum packet size: **45 bytes** (empty payload, no tag).

### 1.2 Flags

| Bit | Mask | Name | v1 meaning |
| --: | :-- | :-- | :-- |
| 0 | `0x0001` | `ENCRYPTED` | Payload is AES-256-GCM ciphertext; a 16-byte tag follows the payload; `sequence` is meaningful. |
| 1 | `0x0002` | `COMPRESSED` | **Reserved.** No v1 implementation compresses. Senders MUST NOT set it; receivers MAY drop packets that have it set. |
| 2 | `0x0004` | `RELIABLE` | **Reserved.** No retransmission layer exists in v1. Same handling as `COMPRESSED`. |

### 1.3 Worked example (from the golden vectors)

Plaintext `TextMessage` "Hello, AdaTP!", session id
`000102030405060708090a0b0c0d0e0f`, timestamp `1700000000000`:

```
54414441 01 0000 0d000000 0000000000000000 2000 0068e5cf8b010000
000102030405060708090a0b0c0d0e0f 48656c6c6f2c20416461545021
│        │  │    │        │                │    │
magic    v  flag len=13   seq=0            type timestamp=0x018bcfe56800
```

## 2. Mapping onto WebSocket

- **W-1** An AdaTP packet MUST be sent as exactly **one WebSocket binary
  message**. Senders MUST NOT concatenate packets in a message and MUST NOT
  split a packet across messages. (WebSocket-level fragmentation of one
  message is transparent and permitted; receivers see the reassembled
  message.)
- **W-2** Receivers MUST ignore WebSocket **text** messages.
- **W-3** WebSocket control frames (Ping/Pong/Close) are handled at the
  WebSocket layer and never contain AdaTP data. Endpoints MUST answer
  WebSocket Pings (standard stacks do this automatically).
- **W-4** The canonical endpoint is path **`/ws`** on port **3000**
  (`ws://host:3000/ws`); production deployments use `wss://` with TLS
  terminated upstream (`../deployment/ports.md`).
- **W-5** Servers MUST bound the accepted message size. The reference
  server rejects payloads above `MAX_FRAME_BYTES` (default 1 MiB) and
  closes the connection (`frame_too_large`).

## 3. Decoding rules

A receiver processing a binary message MUST:

1. Reject messages shorter than 45 bytes (`short_header`).
2. Reject a wrong `magic` (`invalid_magic`).
3. Reject an unknown `version` by closing the connection
   ([10-versioning.md](10-versioning.md)).
4. Reject messages where fewer than `length` payload bytes are present
   (`incomplete_payload`), or where `flags.ENCRYPTED` is set but fewer than
   16 tag bytes follow the payload (`missing_auth_tag`).
5. Trailing bytes beyond `45 + length (+16)` MUST be ignored.

Rejections at steps 1–4 are protocol violations: the receiver SHOULD close
the connection (server close reason `malformed_packet`).

## 4. Encoding rules

An encoder MUST:

1. Set `magic = 0x41444154`, `version = 1`.
2. Set `length` to the exact payload size (the ciphertext size for
   encrypted packets).
3. For encrypted packets: set `flags.ENCRYPTED`, set `sequence` to the
   value used for nonce derivation, and append the 16-byte tag.
4. For plaintext packets: leave `sequence = 0` and append no tag.

`timestamp` SHOULD be the sender's current Unix time in milliseconds;
embedded senders MAY use a monotonic millisecond counter instead.

## 5. Transport independence (informative)

The 45-byte frame is self-describing except for message boundaries, which
v1 delegates to WebSocket. The retired raw-TCP carrier re-derived
boundaries from `length` + `flags`; `adatp-core`'s `TcpTransport` remains
available as library code for embedders (`../legacy.md`), but is outside
the v1 conformance surface.
