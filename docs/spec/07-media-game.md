# AdaTP Specification — 07: Media & Game State

**Status:** Normative, v1.0

---

## 1. Audio

### 1.1 Format

`VoiceData` (0x0044) carries **raw PCM** audio:

| Property | Value |
| :-- | :-- |
| Codec | none (linear PCM) |
| Sample rate | 16 000 Hz |
| Sample format | signed 16-bit integer, **little-endian** (S16LE) |
| Channels | 1 (mono) |
| Frame size | RECOMMENDED 2048 samples (4096 bytes) per packet; senders MAY use other sizes up to `MAX_FRAME_BYTES` |

The payload is the bare sample data — no per-frame header. At 2048
samples per packet this is one packet every 128 ms, ≈ 32 kB/s per speaking
participant.

Rationale (informative): uncompressed PCM keeps clients codec-free
(browsers via WebAudio, ESP32 via I2S) and feeds ML/AI pipelines directly.
The cost is bandwidth; a compressed profile would be a new message type
per [09-extensions.md](09-extensions.md), since the `COMPRESSED` flag is
reserved.

### 1.2 Routing and mixing

- `VoiceData` routes like any routable packet: to every member of the
  sender's room, **including the sender**. Clients MUST NOT play back
  their own frames (compare sender session id).
- The server does not mix, transcode, or drop silence; **N** speakers in a
  room produce **N** inbound streams per member. Clients mix locally.
- Delivery is at-most-once with no retransmission; audio tolerates loss by
  design. Under backpressure the server drops rather than delays
  (`../architecture/reliability.md`).

### 1.3 Voice signaling packets

`VoiceInit/Offer/Answer/Ice/End` (0x0040–0x0043, 0x0045) are routable
envelopes with no server semantics, available to applications that prefer
structured signaling. The v1 reference clients use the text grammar of
[06-signaling.md](06-signaling.md) instead; both are conformant.

## 2. Video (reserved)

The family `0x0090–0x0094` (VideoInit/Offer/Answer/Data/End) is reserved.
The server routes these packets like other routable types but v1 defines
no payload format, and no reference client implements video. A future
minor revision will specify payloads; until then implementations SHOULD
NOT send them.

## 3. GameState (0x0050)

### 3.1 Semantics

`GameState` is the first-class carrier for shared application state —
game boards, lobby rosters, cursors, telemetry. Normative behaviour:

- **G-1** The payload is **opaque to the server**: bytes in, same bytes
  routed out. No validation, no merging, no persistence.
- **G-2** Routing is identical to `TextMessage`: broadcast to the sender's
  room, sender included, at-most-once.
- **G-3** Only authenticated connections may send it (like all routable
  types).
- **G-4** Senders MUST keep each state packet within `MAX_FRAME_BYTES`;
  send deltas or shard large worlds rather than growing single packets.

### 3.2 Recommended envelope (non-normative but interoperable)

Payloads SHOULD be UTF-8 JSON using this envelope:

```json
{ "v": 1, "game": "<game or app id>", "state": { ... } }
```

- `v` — envelope version, integer, currently 1.
- `game` — a short identifier so multiple games/apps can share a room
  without misparsing each other's state.
- `state` — application-defined.

Receivers SHOULD ignore `GameState` packets whose `game` id they do not
recognise, and MUST tolerate payloads that do not parse as JSON (the
envelope is a recommendation, not a wire rule — G-1 governs).

### 3.3 Consistency model (informative)

GameState provides **broadcast, not consensus**:

- Ordering is per-sender only; two players' updates may interleave
  differently at different receivers.
- The last-writer-wins outcome of concurrent full-state broadcasts is
  whatever arrives last at each receiver.
- Games needing authority should nominate one client as host (e.g. the
  room's first member via `DISCOVERY:` of
  [06-signaling.md §3](06-signaling.md)) and have others send *intents*
  that the host folds into authoritative `GameState` broadcasts.
- A late joiner has no history; it SHOULD request state (application
  convention, e.g. a `{"state_request": true}` intent or
  `DISCOVERY:WHO_IS_HERE`) and rely on the host or any peer to re-broadcast.

### 3.4 Worked example (from the golden vectors)

The vector `frame-encrypted-gamestate-server` in
[appendix-test-vectors.md](appendix-test-vectors.md) encodes the payload
`{"board":[1,0,2],"turn":"p1"}` as a server-encrypted `GameState` packet
(seq = 2, `server_write` key). Implementations adding GameState support
SHOULD replay it.
