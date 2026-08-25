# Error Handling

AdaTP fails **loudly and closed**: rejections carry machine-readable
codes, and when the server can't verify something (auth backend down,
tampered ciphertext) it refuses rather than guesses. Full registry:
[`docs/spec/appendix-error-codes.md`](../spec/appendix-error-codes.md).

## AuthFailure payloads

`AuthFailure (0x0014)` carries JSON `{"error":"<code>"}`:

| Code | When | Client reaction |
| :-- | :-- | :-- |
| `invalid_credentials` | wrong username/password | fix credentials; 3 strikes close the connection |
| `auth_unavailable` | the `api` auth backend is unreachable — **fail-closed**, connection closes | retry later with backoff; page your backend |
| `malformed_auth_request` | AuthRequest payload isn't the expected JSON | client bug |
| `forbidden` | a policy plugin vetoed an otherwise-valid login | escalate to the operator |
| `not_authenticated` | you sent traffic before `AuthSuccess` | authenticate first (also sent for pre-auth `JoinRoom` etc.) |
| `invalid_room_name` | JoinRoom name empty/oversized/control chars | fix the name (1–128 chars) |

SDK surfaces: Node `authenticate()` rejects, Python/PHP raise, C returns
negative — all carrying the payload text.

## Connection close reasons

When the server closes, the reason lands in its log (and in
`connection.closed` webhook events). The ones you'll actually meet:

| Reason | Cause |
| :-- | :-- |
| `auth_failed` | three failed logins |
| `auth_unavailable` | verification backend down (fail-closed) |
| `malformed_packet` / `frame_too_large` | bad framing / payload over `MAX_FRAME_BYTES` |
| `decrypt_failed` / `handshake_verify_failed` | bad ciphertext or tampered tag |
| `handshake_replay` / `bad_handshake_key` | protocol misuse during handshake |
| `preauth_flood` | 10+ unauthorized packets before login |
| `idle_timeout` | silent > `IDLE_TIMEOUT_SECS` (90 s; WS pings every 30 s keep healthy clients alive automatically) |
| `server_shutdown` | drain/shutdown — you also receive `Disconnect ("server_shutdown")` first |
| `client_disconnect` / `peer_closed` | you left |

## Tool call errors

`ToolError (0x0072)` → `error.code`
(table in [tools & plugins](tools-and-plugins.md)): `tool_not_found`,
`tool_invalid_args`, `tool_rate_limited`, `tool_timeout`, `tool_failed`,
`tool_forbidden`. Treat `tool_timeout` as *unknown outcome*.

## Retry guidance

- **Connection drops** (network, idle, shutdown): reconnect with jittered
  exponential backoff (1 s → 2 s → … cap ~30 s) and re-run the full
  sequence — handshake, authenticate, join. State is in-memory only;
  after a server restart nothing about you is remembered.
- **`invalid_credentials`** is terminal for those credentials — don't
  hammer; three attempts close the socket anyway.
- **`auth_unavailable`**: backoff generously; the server is protecting
  itself by failing closed.
- **`tool_rate_limited`**: the budget is per-minute; wait for the next
  window.
- **Send failures / full queues**: the server drops rather than stalls
  (per-connection queue of 256). Voice/state traffic should just keep
  streaming; for must-arrive text, add an application-level ack
  (`TextAck (0x0021)` is reserved for exactly this convention).

## Client-side hygiene

- Always handle `auth_failure` in the browser SDK (`chat.on('auth_failure', …)`)
  — the socket stays open but nothing will route.
- Wait for confirmations (`auth` event, `RoomJoined`) before sending into
  a room; the SDK helpers do this.
- Expect interleaving: never assume "the next packet" is your reply —
  wait by type (see [protocol guide](protocol-guide.md)).
- Log the JSON error codes, not just message strings — they're the
  stable contract.
