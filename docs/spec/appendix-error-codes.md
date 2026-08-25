# AdaTP Specification — Appendix: Error Codes

**Status:** Normative, v1.0

AdaTP signals failure on three channels:

1. **`AuthFailure` packets** (0x0014) carrying JSON `{"error":"<slug>"}` —
   the connection usually stays open (exceptions noted).
2. **`ToolError` packets** (0x0072) carrying
   `{"id","tool","ok":false,"error":{"code","message"}}`.
3. **Connection-close reasons** — the server terminates the WebSocket and
   records the slug in its structured log (`Connection <peer> closed
   (<reason>)`). Close reasons are not transmitted as packets (except
   `server_shutdown`, which is preceded by a `Disconnect` packet).

Implementations MUST use these exact slugs.

---

## 1. `AuthFailure` error codes (JSON channel)

| Code | Emitted when | Connection |
| :-- | :-- | :-- |
| `invalid_credentials` | `AuthRequest` verified and rejected by the driver (wrong username/password, or the external API said no). | stays open; auth attempt counter +1; closes as `auth_failed` on the 3rd failure |
| `malformed_auth_request` | `AuthRequest` payload is not valid JSON with string `username`/`password`. | stays open; counts as a failed attempt |
| `auth_unavailable` | The verification backend errored (API unreachable/misbehaving, user file unreadable). Fail-closed. | **closed** immediately after sending (`auth_unavailable`) |
| `not_authenticated` | Any routable packet, `JoinRoom`, or `ToolCall` received before `AuthSuccess`. | stays open; pre-auth violation counter +1; closes as `preauth_flood` on the 10th |
| `invalid_room_name` | `JoinRoom` payload is not UTF-8, empty, longer than 128 bytes, or contains control characters. | stays open |

## 2. Tool error codes (`ToolError` / `TOOLRESULT:` channel)

| Code | Emitted when |
| :-- | :-- |
| `tool_not_found` | No registered/enabled tool has the requested name. |
| `tool_timeout` | The tool did not finish within its execution deadline. |
| `tool_rate_limited` | The caller exceeded the tool's configured rate limit. |
| `tool_invalid_args` | The call's `args` failed the tool's JSON-Schema validation, or the `ToolCall` body itself was malformed. |
| `tool_failed` | The tool executed and returned/raised an error. |
| `tool_forbidden` | The caller's role is not allowed to invoke the tool (default-deny permissions). |

The connection always stays open after a `ToolError`.

## 3. Connection-close reasons (log channel)

| Reason | Meaning |
| :-- | :-- |
| `malformed_packet` | Frame failed the decoding rules of [03-framing.md §3](03-framing.md) (bad magic, short header, truncated payload/tag, mid-stream version change). |
| `frame_too_large` | Message/payload exceeded `MAX_FRAME_BYTES`. |
| `decrypt_failed` | An `ENCRYPTED` packet failed AES-GCM authentication after session establishment, or arrived with no session. |
| `handshake_verify_failed` | `HandshakeComplete` tag verification failed — key agreement broken. |
| `handshake_replay` | `HandshakeInit` received on a connection that already handshook or authenticated. |
| `bad_handshake_key` | The client's X25519 public key was unusable for Diffie–Hellman. |
| `auth_failed` | Third failed authentication attempt. |
| `auth_unavailable` | Backend failure during verification (fail-closed close; paired with the JSON code above). |
| `preauth_flood` | Tenth refused pre-auth packet. |
| `idle_timeout` | No inbound traffic (packets, WS pongs, anything) for `IDLE_TIMEOUT_SECS` (default 90 s). |
| `peer_closed` | The client closed the WebSocket (close frame or EOF). |
| `client_disconnect` | The client sent an AdaTP `Disconnect` packet (graceful). |
| `server_shutdown` | Graceful server shutdown/drain: a `Disconnect` packet with payload `server_shutdown` is sent first, then the socket closes. |

Reference-server internals may additionally log transport-level reasons
(`read_error`, `write_error`, `queue_closed`); these indicate socket/IO
failure rather than protocol violations and are not part of the normative
registry.

## 4. Client guidance (informative)

- Treat `invalid_credentials` as user-facing ("wrong password"),
  `auth_unavailable` as operational ("try later"), and
  `not_authenticated` as a client bug (traffic sent too early).
- After `decrypt_failed`-class closes, re-connect and re-handshake from
  scratch; session keys are never resumable.
- SDKs SHOULD surface close reasons verbatim in errors/logs so operator
  logs and client logs correlate.
