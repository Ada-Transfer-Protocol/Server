# Voice

AdaTP ships audio as **raw PCM** — deliberately "dumb" so any client, DSP
chain, or AI pipeline can consume it without codec plumbing.

## Audio format

| Property | Value |
| :-- | :-- |
| Codec | none — raw PCM |
| Sample rate | 16 000 Hz |
| Sample format | signed 16-bit little-endian (S16LE) |
| Channels | 1 (mono) |
| Typical frame | 2048 samples = 4096 bytes per `VoiceData` packet |

At 16 kHz mono that's ~32 kB/s per speaking participant before header
overhead. `VoiceData (0x0044)` is room-routed like any other packet; the
server **forwards, never mixes** — each client receives every speaker's
stream separately and mixes locally (the browser SDK just plays each
buffer as it lands).

The rest of the voice family (`VoiceInit/Offer/Answer/Ice/End`,
0x0040–0x0045) is routed verbatim for client-side call setup protocols;
the reference clients only use `VoiceData`.

## Why raw PCM is AI-friendly

16 kHz S16LE mono is the native input of most speech models (Whisper,
wav2vec-family, common TTS output). An agent can pipe `VoiceData`
payloads straight into an STT model and answer with synthesized PCM —
no Opus decode, no resampling in the common case. See the
[AI agent guide](AI_AGENT_DEVELOPMENT.md).

## Group calls — `AdaTPConference`

The browser conference keeps peer state without server bookkeeping, using
text-message **discovery**:

1. On `join(room)` the newcomer broadcasts `DISCOVERY:WHO_IS_HERE`.
2. Existing peers answer `DISCOVERY:I_AM_HERE` (and re-announce their mute
   state) — the newcomer builds its user set from the answers.
3. A leaver broadcasts `DISCOVERY:I_AM_LEAVING` before dropping.
4. Mute toggles broadcast `MUTE:ON` / `MUTE:OFF`.

Voice activity is inferred from receiving someone's `VoiceData`.

## 1:1 calls — `AdaTPPhone`

Call control is text-message signaling in a shared signaling room
(`global_signaling`), with the media session in a per-call room:

| Message | Meaning |
| :-- | :-- |
| `INVITE:<targetId>:<room>` | ring `targetId`, proposing media room `<room>` |
| `RINGING:<room>` | callee's device is alerting |
| `ACCEPT:<room>` | callee accepted — both join `<room>` and start audio |
| `REJECT:<room>` | declined |
| `BUSY:<room>` | callee already in a call |
| `BYE` | terminate (sent inside the media room) |

`<targetId>` matches the first 6 hex chars of the callee's session id (as
shown by `getMyId()` in the demo pages).

## Heartbeat / RTT

`SYS:PING` is a text message the client sends to its **own room**; because
room broadcasts include the sender, the client times its own echo —
that round trip (client → server → back) is the latency figure surfaced
as `onNetworkQuality`. There's also a binary `Ping (0x0080)` /
`Pong (0x0081)` pair answered by the server directly.

These prefixes — `SYS:`, `DISCOVERY:`, `MUTE:`, `TOOL:`, `TOOLRESULT:`,
plus the call-control verbs — are **reserved signaling vocabulary**
([spec §6](../spec/06-signaling.md)); don't repurpose them for chat.

## Building voice clients outside the browser

Any SDK can do voice: capture 16 kHz S16LE mono frames and send them with
the generic send API (`adatp_client_send(c, 0x0044, buf, len)` in C,
`_send_encrypted(MessageType.VOICE_DATA, …)` in Python), and play
received `VoiceData` payloads. Keep frames at 1024–4096 samples; under
the default `MAX_FRAME_BYTES` (1 MiB) this is never a concern.

Bandwidth math for rooms: one speaker in an N-member room costs the
server N outbound streams (~32 kB/s each). Prefer several small rooms
over one giant one; see [reliability](../architecture/reliability.md).
