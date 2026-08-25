# AdaTP Specification — 06: Signaling

**Status:** Normative, v1.0

AdaTP carries lightweight signaling as plain UTF-8 payloads inside
`TextMessage` (0x0020) packets. This keeps minimal clients (browsers,
microcontrollers) free of extra packet types: anything that can send text
can signal. The server routes these like any text — it assigns **no**
semantics to them; the grammar below is a contract **between clients**.

---

## 1. General rules

- **S-1** A signaling message is the entire `TextMessage` payload matching
  one of the productions below. Case-sensitive; fields are separated by
  `:` (colon); field values MUST NOT contain `:` unless the production
  says otherwise.
- **S-2** Clients MUST ignore signaling messages they do not understand
  (forward compatibility).
- **S-3** Because routable packets echo to the sender
  ([02-architecture.md §6](02-architecture.md)), clients MUST be prepared
  to receive their own signaling messages and use the sender session id to
  tell self from others.
- **S-4** Signaling messages are only meaningful within the room they are
  sent to. Call-control conventionally happens in a well-known signaling
  room (reference clients use `global_signaling`), with media moving to a
  per-call room afterwards.

## 2. Call control (1:1 calls)

| Message | Direction | Meaning |
| :-- | :-- | :-- |
| `INVITE:<target>:<room>` | caller → room | Invite `<target>` (session-id prefix or full id, hex) to the media room `<room>`. |
| `RINGING:<room>` | callee → room | The callee's device is alerting for the call identified by `<room>`. |
| `ACCEPT:<room>` | callee → room | Call accepted; both sides join `<room>` and start media. |
| `REJECT:<room>` | callee → room | Call declined. |
| `BUSY:<room>` | callee → room | Callee is already in a call. |
| `BYE` | either → media room | Terminate the current call; sent inside the media room. |

`<room>` is the call correlation token: reference clients generate
`room_<unix-millis>` and match subsequent RINGING/ACCEPT/REJECT/BUSY
messages against the pending room name.

Call flow (informative):

```
caller                       signaling room                        callee
  │ INVITE:ab12cd:room_17...  ───────────────────────────────────►  │
  │  ◄───────────────────────────────────  RINGING:room_17...       │
  │  ◄───────────────────────────────────  ACCEPT:room_17...        │
  │            (both send JoinRoom "room_17..." and stream)         │
  │ BYE  ─────────────── (inside room_17...) ─────────────────────► │
```

## 3. Conference discovery (group rooms)

Peer discovery without server bookkeeping — every participant answers for
itself:

| Message | Sent by | Meaning |
| :-- | :-- | :-- |
| `DISCOVERY:WHO_IS_HERE` | a newcomer, after joining | Ask present members to identify themselves. |
| `DISCOVERY:I_AM_HERE` | every member, in response | "I exist" — receivers add the sender id to their roster. |
| `DISCOVERY:I_AM_LEAVING` | a member, before leaving | Graceful roster removal (complements the server's `PresenceUpdate "LEAVE"`). |

## 4. Media state

| Message | Meaning |
| :-- | :-- |
| `MUTE:ON` | Sender muted its microphone. |
| `MUTE:OFF` | Sender unmuted. |

## 5. Liveness / RTT

| Message | Meaning |
| :-- | :-- |
| `SYS:PING` | RTT probe. The sender measures the time until **its own echo** returns from the server. No peer replies; there is no `SYS:PONG`. |

Clients that prefer a binary probe use `Ping`/`Pong`
(0x0080/0x0081, [04-packets.md §2.9](04-packets.md)) instead; `SYS:PING`
exists so that pure-text clients can measure RTT.

## 6. Tool-call fallback (`TOOL:` / `TOOLRESULT:`)

Clients that cannot emit the binary tool packets
([09-extensions.md](09-extensions.md)) MAY use the text fallback:

- **Request:** a `TextMessage` whose UTF-8 payload starts with the exact
  prefix `TOOL:` followed immediately by the `ToolCall` JSON object:

  ```
  TOOL:{"id":"call-7","tool":"echo","args":{"text":"hi"}}
  ```

- **Response:** the server replies — **to the caller only**, never
  broadcast — with a `TextMessage` starting with `TOOLRESULT:` followed by
  the `ToolResult` or `ToolError` JSON:

  ```
  TOOLRESULT:{"id":"call-7","tool":"echo","ok":true,"result":{"text":"hi"}}
  TOOLRESULT:{"id":"call-7","tool":"echo","ok":false,"error":{"code":"tool_timeout","message":"..."}}
  ```

Rules:

- **S-5** A server supporting tools MUST intercept `TOOL:`-prefixed
  `TextMessage`s from authenticated clients and MUST NOT broadcast them to
  the room.
- **S-6** The JSON after the prefix MUST be exactly the schemas of
  [09-extensions.md §2](09-extensions.md); a malformed body is answered
  with `TOOLRESULT:` + `ToolError` (`tool_invalid_args`).
- **S-7** Clients MUST correlate by `id`, not by ordering.

## 7. Reserved prefixes

The prefixes `SYS:`, `TOOL:`, `TOOLRESULT:`, `DISCOVERY:`, `MUTE:`,
`INVITE:`, `RINGING:`, `ACCEPT:`, `REJECT:`, `BUSY:` and the bare token
`BYE` are reserved for this grammar. Applications SHOULD NOT invent new
meanings for them; new signaling SHOULD use a distinct prefix
(e.g. `X-MYAPP:`), which the grammar's ignore-unknown rule (S-2) keeps
safe.
