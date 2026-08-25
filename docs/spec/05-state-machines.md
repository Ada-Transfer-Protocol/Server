# AdaTP Specification — 05: State Machines

**Status:** Normative, v1.0

This document defines the per-connection state machine a conforming server
MUST implement, and the client-side expectations that follow from it.
Packet payloads are defined in [04-packets.md](04-packets.md); error codes
in [appendix-error-codes.md](appendix-error-codes.md).

---

## 1. Server-side connection states

```
                        ┌────────────────────────────────────────────┐
                        │              (WebSocket open)              │
                        ▼                                            │
                 ┌────────────┐   HandshakeInit(≥32B key)            │
                 │  PRE-AUTH  │ ─────────────────────────┐           │
                 │ (plaintext)│                          ▼           │
                 └─────┬──────┘                  ┌──────────────┐    │
                       │                         │ HANDSHAKING  │    │
                       │ AuthRequest             │ (keys derived│    │
                       │ (plaintext)             │  tag pending)│    │
                       │                         └──────┬───────┘    │
                       │                HandshakeComplete│(tag OK)   │
                       │                                 ▼           │
                       │                         ┌──────────────┐    │
                       │       AuthRequest       │  PRE-AUTH    │    │
                       │       (encrypted)       │  (secure)    │    │
                       │                         └──────┬───────┘    │
                       ▼                                ▼            │
                 ┌─────────────────────────────────────────┐         │
                 │            AUTHENTICATED                │         │
                 │  room = "global" → JoinRoom moves it    │         │
                 └────────────────────┬────────────────────┘         │
                                      │ Disconnect / close /         │
                                      │ idle / error / shutdown      │
                                      ▼                              │
                                 ┌─────────┐ ────────────────────────┘
                                 │ CLOSED  │   (PresenceUpdate "LEAVE"
                                 └─────────┘    to last room, if authed)
```

State summary:

| State | Meaning |
| :-- | :-- |
| **PRE-AUTH (plaintext)** | Connected; no keys, no identity verified. |
| **HANDSHAKING** | Server answered `HandshakeInit` and derived keys; waiting for the first valid encrypted packet. |
| **PRE-AUTH (secure)** | Encrypted channel established (`HandshakeComplete` tag verified); identity still unverified. |
| **AUTHENTICATED** | `AuthSuccess` sent; connection registered, member of a room. |
| **CLOSED** | Terminal. Cleanup ran: unregister + `PresenceUpdate "LEAVE"`. |

## 2. Event × state table

"refused" = server replies `AuthFailure {"error":"not_authenticated"}` and
increments the pre-auth violation counter. "ignored" = dropped silently.

| Event | PRE-AUTH | HANDSHAKING | AUTHENTICATED |
| :-- | :-- | :-- | :-- |
| `HandshakeInit` ≥ 32 B key | derive keys → HANDSHAKING; reply `HandshakeResponse(server key)` | close `handshake_replay` | close `handshake_replay` |
| `HandshakeInit` < 32 B | reply empty `HandshakeResponse`; stay (plaintext ack) | close `handshake_replay` | close `handshake_replay` |
| `HandshakeComplete` (ENCRYPTED, tag OK) | ignored (no keys) | → PRE-AUTH (secure) | ignored |
| `HandshakeComplete` (tag bad) | ignored | close `handshake_verify_failed` | ignored |
| `AuthRequest` valid credentials | → AUTHENTICATED; reply `AuthSuccess` | *(processed after completion in practice; same as PRE-AUTH)* | re-verify; on success update identity, reply `AuthSuccess` |
| `AuthRequest` bad credentials | reply `AuthFailure invalid_credentials`; attempt++ ; ≥3 → close `auth_failed` | same | same |
| `AuthRequest` malformed JSON | reply `AuthFailure malformed_auth_request`; attempt++ | same | same |
| `AuthRequest`, backend down | reply `AuthFailure auth_unavailable`; close `auth_unavailable` (fail closed) | same | same |
| `Ping` | reply `Pong` (echo) | reply `Pong` | reply `Pong` |
| `Disconnect` | close `client_disconnect` | same | same |
| `JoinRoom` | **refused** | **refused** | validate name → move rooms; reply `RoomJoined`; presence announcements. Invalid name → `AuthFailure invalid_room_name`, stay. |
| Routable packet ([04-packets.md §1](04-packets.md)) | **refused** | **refused** | decrypt if needed → broadcast to room (sender included) |
| `ToolCall` | **refused** | **refused** | dispatch to tool layer; reply `ToolResult`/`ToolError` to caller only |
| Reserved/unknown type | ignored | ignored | ignored |
| Undecryptable ENCRYPTED packet | close `decrypt_failed` | close `decrypt_failed` | close `decrypt_failed` |
| Malformed frame ([03-framing.md §3](03-framing.md)) | close `malformed_packet` | same | same |
| Oversized frame | close `frame_too_large` | same | same |
| WS close from peer | close `peer_closed` | same | same |
| Idle > `IDLE_TIMEOUT_SECS` | close `idle_timeout` | same | same |
| Server drain/shutdown | send `Disconnect "server_shutdown"`; close | same | same |

## 3. Counters

A conforming server MUST maintain per connection:

- **auth_attempts** — incremented on every failed or malformed
  `AuthRequest`. When it reaches **3**, the connection MUST be closed
  (`auth_failed`).
- **preauth_violations** — incremented on every refused packet before
  authentication. When it reaches **10**, the connection MUST be closed
  (`preauth_flood`).

Both counters exist to bound the work an unauthenticated peer can cause.

## 4. Sequencing and ordering guarantees

- Packets from one connection are processed strictly in arrival order
  (WebSocket preserves message order). A client MAY therefore pipeline
  `AuthRequest` followed immediately by `JoinRoom`; the server processes
  them in order.
- Encrypted-session sequence numbers are per **direction**: the client's
  counter and the server's counter both start at 1 and increment
  independently ([08-security.md §4](08-security.md)).
- Across *different* senders no ordering is guaranteed
  (`../architecture/reliability.md`).

## 5. Client-side expectations (normative for SDKs)

- A client MUST treat the response stream as multiplexed: while waiting
  for a specific reply (`AuthSuccess`, `RoomJoined`, `ToolResult` by
  correlation id), unrelated packets (presence, chat, voice) MAY arrive
  first and MUST NOT be discarded as protocol errors. Reference SDKs
  queue them.
- A client MUST NOT send routable traffic before receiving `AuthSuccess`,
  except that pipelining directly after `AuthRequest` is permitted (the
  refusals it may cause count against its own violation budget).
- After sending `HandshakeInit` with a key, a client MUST NOT send
  plaintext packets other than nothing until `HandshakeComplete`; after
  completion it SHOULD encrypt everything.
- A client receiving `Disconnect` SHOULD close the WebSocket without
  waiting for further traffic.

## 6. Handshaking edge (informative)

In the reference server the HANDSHAKING → PRE-AUTH (secure) edge is
lenient: an `AuthRequest` arriving between `HandshakeResponse` and
`HandshakeComplete` is processed (the reply is encrypted only once the
tag-verified `HandshakeComplete` has arrived). Clients SHOULD nevertheless
complete the handshake before authenticating — all reference SDKs do.
