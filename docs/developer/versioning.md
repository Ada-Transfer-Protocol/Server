# Versioning

Normative policy: [`docs/spec/10-versioning.md`](../spec/10-versioning.md).
This page is what client authors need day-to-day.

## The version byte

Header offset 4 carries the protocol version — **`1`** for everything
this documentation set describes. Rules of engagement:

- Always send `1`.
- A server receiving an unknown version MUST close; expect the same
  courtesy from future servers toward v1 (or an explicit compatibility
  note in their spec).
- There is **no negotiation handshake** in v1 — the byte is a declaration,
  not an offer. That's an honest, documented limitation.

## What changes without a version bump

Additive, backward-safe evolution:

- **New message types.** Unknown types are ignored by the server and
  should be ignored by clients — never crash on an unrecognized
  `msg_type`.
- **New JSON fields** in payloads (AuthSuccess identity, tool listings,
  webhook envelopes…). Parse leniently; ignore unknown keys.
- New tools, plugins, webhook events, admin endpoints.

What forces a bump: header layout changes, semantic changes to existing
types, crypto scheme changes.

## The pre-1.0 renumbering (one-time migration note)

If you carry code from pre-release AdaTP, four ranges moved when
GameState and the tool platform landed:

| Code | Was (pre-release) | Is (v1) |
| :-- | :-- | :-- |
| `0x0050`–`0x0054` | VideoInit…VideoEnd | **`0x0050` = GameState**; video moved |
| `0x0070` / `0x0071` | Ping / Pong | **ToolCall / ToolResult** (+`0x0072` ToolError) |
| `0x0080` / `0x0081` | — | **Ping / Pong** (relocated) |
| `0x0090`–`0x0094` | — | Video family (reserved) |

All v1.0.0 SDKs, the server, the spec and the golden vectors agree on the
right-hand column. Symptom of a stale client: "pings" being answered
with `tool_invalid_args`-style errors, or game state parsing as video.
Fix: upgrade the SDK; the wire header itself didn't change.

## Version numbers around the project

- **Protocol**: the version byte (`1`) — the only wire-level contract.
- **Spec**: `docs/spec/` is versioned with the release (v1.0).
- **Server / SDK packages**: semver `1.0.0` at release; package versions
  move independently of the protocol byte (a 1.x server still speaks
  protocol 1).
- **Your payloads**: version them yourself — the GameState envelope's
  `"v"` field is the worked example.

## Compatibility promises for 1.x

- The 45-byte header layout and the v1 type registry are frozen.
- Registered types keep their semantics; reserved types
  (`TextAck`, `TextRead`, `FileAck`, `FileCancel`, video, auth challenge
  flow) can gain semantics later without renumbering.
- Extension code points for experiments: `0x0100+` (experimental) and
  `0xFF00–0xFFFE` (private use) — see
  [`docs/spec/09-extensions.md`](../spec/09-extensions.md).
