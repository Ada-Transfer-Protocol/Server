# AdaTP Specification — 02: Architecture

**Status:** Normative, v1.0

---

## 1. Model

AdaTP is a hub-and-spoke protocol:

```
 browser ──ws──┐
 node app ─ws──┤                        ┌── plugin / tool layer
 python  ──ws──┼──►  AdaTP server  ─────┤
 esp32   ──ws──┤   (rooms + routing)    └── webhook dispatch (egress)
 c client ─ws──┘
                       │
                HTTP control plane
        /healthz /readyz /api/* (/admin/v1 reserved)
```

- **Clients** open one WebSocket each to the server's `/ws` endpoint.
- The **server** authenticates each connection, assigns it to exactly one
  room, and routes routable packets to the room's members.
- Servers do not federate in v1: a room exists in exactly one server
  process (see `../architecture/reliability.md`).

## 2. Data plane vs control plane

| Plane | Carrier | Contents |
| :-- | :-- | :-- |
| **Data plane** | WebSocket `/ws`, binary AdaTP packets | handshake, auth, rooms, text, files, voice, game state, tool calls |
| **Control plane** | HTTP on the same port | `/healthz`, `/readyz`, `/api/status`, `/api/metrics` (API-key protected); `/admin/v1/*` is reserved for the admin control plane |

Control-plane activity MUST NOT block data-plane routing: webhook delivery,
plugin administration and UI queries run outside the per-connection packet
path.

## 3. Connection anatomy (server side)

Each accepted WebSocket becomes an independent task owning:

- the socket (read + write),
- the connection's protocol state (see
  [05-state-machines.md](05-state-machines.md)),
- the optional secure-session cipher state, and
- a bounded outbound queue (capacity 256 events) that other connections'
  broadcasts are delivered into.

Routing between connections passes **plaintext** payloads internally; the
receiving connection encrypts with **its own** session keys (or sends
as-is on plaintext sessions). This is what makes mixed rooms — a browser in
plaintext next to an encrypted ESP32 — interoperate, and is also why the
AdaTP encryption layer is *not* end-to-end
([08-security.md](08-security.md)).

## 4. Identity

- Every client chooses a random 16-byte **session id** and places it in
  every header it sends.
- The server pins the session id from the **first** packet received on a
  connection: later packets with a different id do not change the
  connection's identity. A nil (all-zero) id causes the server to assign a
  random one.
- Packets routed to room members carry the **sender's** session id, so
  receivers can attribute traffic. Server-originated replies
  (`AuthSuccess`, `RoomJoined`, `Pong`, …) are stamped with the
  *recipient's* own session id.
- The session id is an identity **label**, not a credential. Account
  identity comes from authentication (`user_id`, `username`, `role` in
  `AuthSuccess`).

## 5. Rooms

- After `AuthSuccess`, the connection is a member of room **`global`**.
- `JoinRoom` moves the connection; the server confirms with `RoomJoined`.
  Membership is exclusive: joining room B removes the connection from
  room A.
- Room names MUST be valid UTF-8, 1–128 bytes, containing no control
  characters. Invalid names are refused with error `invalid_room_name`
  (connection stays open).
- On departure (join elsewhere or disconnect), the old room receives
  `PresenceUpdate "LEAVE"` stamped with the departing session id. On
  arrival, the new room — excluding the joiner — receives
  `PresenceUpdate "JOIN"`.
- Empty rooms cease to exist; rooms are created implicitly by joining
  them. There is no room directory, ACL, or persistence in v1.

## 6. Routing rules

A packet type is **routable** if it belongs to the text, file, voice,
video, game-state, presence, or typing families
([04-packets.md](04-packets.md)). For a routable packet from an
authenticated connection, the server MUST:

1. decrypt it with the sender's session (if `ENCRYPTED`),
2. re-encode it per recipient (encrypting with each recipient's session
   where one is established), preserving `msg_type` and the sender's
   session id,
3. deliver it to **every** member of the sender's room, including the
   sender.

Non-routable types (`ToolCall`, handshake, auth, `JoinRoom`, `Ping`,
`Disconnect`) are consumed by the server. Unknown-but-parseable types are
ignored (dropped without error) — see
[09-extensions.md](09-extensions.md) for forward-compatibility rules.

Delivery is **best-effort, at-most-once**: a recipient whose outbound
queue is full loses that packet (counted in the `dropped_messages`
metric). AdaTP defines no retransmission; see
`../architecture/reliability.md`.

## 7. Session lifecycle overview

```
connect ─► [optional X25519 handshake] ─► authenticate ─► room "global"
        ─► JoinRoom* / traffic ─► Disconnect | close | idle timeout
```

1. **Connect.** Client opens `ws(s)://host:port/ws`.
2. **Handshake (optional).** `HandshakeInit` (client X25519 public key) →
   `HandshakeResponse` (server key) → `HandshakeComplete` (first encrypted
   packet). Skipping this leaves the session plaintext.
3. **Authenticate.** `AuthRequest` with JSON credentials →
   `AuthSuccess` or `AuthFailure`. Until success, only handshake packets,
   `AuthRequest`, `Ping` and `Disconnect` are accepted.
4. **Traffic.** Join rooms, exchange routable packets, invoke tools.
5. **Teardown.** Client `Disconnect`, transport close, idle timeout, or
   server shutdown (`Disconnect` with reason `server_shutdown`). The
   server always announces `PresenceUpdate "LEAVE"` to the last room.

The full state machine with error transitions is in
[05-state-machines.md](05-state-machines.md).

## 8. Liveness

The server sends a WebSocket protocol-level Ping every 30 s and closes
connections with no inbound activity for `IDLE_TIMEOUT_SECS` (default
90 s). WebSocket stacks answer protocol pings automatically, so an
otherwise-idle listener stays alive without application traffic.
Application-level `Ping` (0x0080) / `Pong` (0x0081) exists for RTT
measurement and works in every state.
