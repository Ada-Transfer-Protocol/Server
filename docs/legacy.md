# Legacy: the raw-TCP listener (:8444)

Before v1.0, the AdaTP server exposed two listeners:

- an HTTP + WebSocket listener on `:3000`, and
- a **raw TCP listener on `:8444`** speaking AdaTP framing directly over TCP.

## Status: removed in v1.0

The raw-TCP listener has been **removed from the server**. It is no longer
started under any configuration. The reasons:

1. **One canonical transport.** All six official SDKs now speak WebSocket to
   `:3000` (`/ws`). Maintaining two data-plane listeners doubled the attack
   surface and the test matrix.
2. **The TCP path had placeholder auth.** The pre-1.0 TCP handler accepted
   any `AuthRequest` without verifying credentials and sent a mock (all-zero)
   handshake key. Rather than ship a second, weaker code path, it was
   deleted; the WebSocket path implements the real handshake and real
   credential verification.
3. **Proxy friendliness.** WebSocket traverses load balancers, Cloudflare and
   corporate proxies; a bespoke TCP port does not.

## What remains

- `adatp-core::transport::tcp::TcpTransport` stays in the `core` library as
  reusable framing code for embedders who need AdaTP over a raw stream
  (e.g. private links). It is **not** wired to the server and is not part of
  the supported v1 surface.
- The wire framing itself (the 45-byte header) is transport-independent and
  unchanged; only the carrier moved to WebSocket.

## Migrating a pre-1.0 TCP client

1. Open a WebSocket to `ws://<host>:3000/ws` instead of a TCP socket to
   `:8444`.
2. Send each AdaTP packet as one binary WebSocket message (do not stream
   packets back-to-back).
3. Everything else — header layout, handshake, key derivation, encryption —
   is identical. All official SDKs already do this; upgrading the SDK is
   sufficient.
