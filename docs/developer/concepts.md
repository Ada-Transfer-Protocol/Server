# Concepts

The five ideas that make everything else make sense.

## Packets

All AdaTP traffic is **binary packets**: a fixed 45-byte header + payload
(+ a 16-byte auth tag when encrypted), carried as **exactly one WebSocket
binary message per packet**. The header names the message type
(`TextMessage`, `VoiceData`, `GameState`, …), the payload is type-specific
(UTF-8, JSON, or raw bytes). Details: [protocol guide](protocol-guide.md).

## Rooms

Routing is room-scoped:

- Every authenticated connection is in **exactly one room** at a time.
- After `AuthSuccess` you start in the default room **`global`**.
- `JoinRoom` moves you; the server confirms with `RoomJoined`. Joining also
  announces you: the old room gets `PresenceUpdate "LEAVE"`, the new room
  (everyone but you) gets `PresenceUpdate "JOIN"`.
- Routable traffic (text, voice, files, game state, presence, typing) is
  broadcast to every member of your current room — **including you**.

That self-echo is a feature: clients confirm delivery order and measure
RTT with it (the browser phone sends `SYS:PING` and times its own echo).
Filter your own packets by comparing the sender's session id with yours.

Room names are 1–128 characters, no control characters. Rooms come into
existence when someone joins and vanish when the last member leaves —
there is nothing to create or delete.

## Session identity

The 16-byte `session_id` in every header is the sender's identity for
routing. The **client chooses it** (a random UUID) and the server **pins**
it to the first packet of the connection — you cannot switch identity
mid-stream. Every packet routed to a room carries the *original sender's*
session id, which is how `senderId` reaches your message handlers.

Identity ≠ authentication: who you *are* to the auth system is the
username you log in with; the session id is a per-connection address.

## Plaintext vs secure sessions

Two session flavors share one wire format:

| | Plaintext | Secure |
| :-- | :-- | :-- |
| Handshake | none (connect → `AuthRequest`) | `HandshakeInit`/`Response`/`Complete` (X25519) |
| Payloads | as-is | AES-256-GCM per packet, `ENCRYPTED` flag + tag |
| Used by | browser SDK | Node, Python, PHP, C, Arduino SDKs |

Secure sessions are **client↔server transport encryption** (the server
decrypts to route and re-encrypts per recipient) — not end-to-end, and the
handshake is unauthenticated. Production always sits behind `wss://` (TLS);
see [`docs/spec/08-security.md`](../spec/08-security.md) for the honest
threat model. Mixed rooms are fine: a plaintext browser and an encrypted
Python client chat happily; the server re-encodes per recipient.

## Control plane vs data plane

The WebSocket at `/ws` is the **data plane** — the hot path for messages,
voice and files. Everything operational lives on the same port but on HTTP
paths: `/healthz`, `/readyz`, `/api/*` (metrics, API-key protected),
`/admin/v1/*` (admin token) and `/silo` (operator UI). Webhooks and plugins
are wired so they **never block the data plane**: webhook delivery is an
async queue, plugin veto hooks have hard timeouts.

## Presence

`PresenceUpdate (0x0060)` carries `"JOIN"`, `"LEAVE"` or `"BUSY"`. The
server emits JOIN/LEAVE on room changes and disconnects; clients may also
broadcast their own presence. The browser conference class additionally
uses `DISCOVERY:*` text messages to enumerate peers on arrival — see
[voice](voice.md).

## The authentication gate

Until `AuthSuccess`, the server accepts only handshake packets,
`AuthRequest`, `Ping` and `Disconnect`. Anything else gets
`AuthFailure {"error":"not_authenticated"}` and is never routed. Three
failed logins close the connection; if the auth backend is down the server
**fails closed**. Details: [error handling](error-handling.md).
