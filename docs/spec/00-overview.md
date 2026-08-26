# AdaTP Specification — 00: Overview

**Status:** Normative, v1.0
**Applies to:** AdaTP protocol version 1 (header `version = 1`)

---

## 1. What AdaTP is

The **Ada Transfer Protocol (AdaTP)** is a lightweight, binary, room-oriented
realtime protocol for text messaging, file transfer, voice, shared game
state, and tool invocation. It is carried over **WebSocket** (RFC 6455)
using binary messages, with a fixed 45-byte packet header and an optional
AES-256-GCM encryption layer negotiated per connection.

AdaTP is a **client–server** protocol. Clients connect to a server,
authenticate, join a *room*, and exchange packets that the server routes to
the other members of the same room. The server is authoritative for
authentication, room membership, and routing; payload contents (chat text,
audio frames, game state) are opaque to the server except where this
specification assigns them semantics.

## 2. Design goals

1. **Deterministic wire format.** Every packet has the same fixed header;
   all integers are little-endian; there is exactly one packet per
   WebSocket message. A conforming decoder can be written from
   [03-framing.md](03-framing.md) alone and verified against the golden
   vectors in [appendix-test-vectors.md](appendix-test-vectors.md).
2. **Proxy-friendly transport.** WebSocket traverses load balancers, TLS
   terminators, and Cloudflare unchanged. AdaTP defines no transport of its
   own. (A pre-1.0 raw-TCP carrier existed and was removed; see the
   workspace `docs/legacy.md`.)
3. **Explicit errors.** Failures are signalled either by an `AuthFailure`
   packet carrying a JSON error code, by a `ToolError` packet, or by a
   controlled connection close with a documented reason. The registry lives
   in [appendix-error-codes.md](appendix-error-codes.md).
4. **Honest security.** The optional encryption layer is *transport*
   security between client and server — not end-to-end encryption — and its
   handshake is unauthenticated. The limits are stated plainly in
   [08-security.md](08-security.md); production deployments MUST use TLS
   (`wss://`).
5. **Extensibility without forking.** Tool packets, plugin-emitted events,
   and reserved code-point ranges give integrators room to grow;
   see [09-extensions.md](09-extensions.md).
6. **Small enough for microcontrollers.** The reference SDK set includes an
   ESP32 client; nothing in the core protocol requires more than a WebSocket
   client, SHA-256/HKDF, X25519, and AES-GCM.

## 3. Terminology

| Term | Meaning |
| :-- | :-- |
| **Packet** | One AdaTP unit: 45-byte header + payload + optional 16-byte auth tag. |
| **Frame** | The WebSocket binary message carrying exactly one packet. |
| **Session id** | The 16-byte identity a client places in its packet headers; pinned by the server to the value in the first packet received. |
| **Room** | The routing scope. A connection is in exactly one room at a time; default `global`. |
| **Secure session** | The AES-256-GCM channel established by the X25519 handshake. |
| **Plaintext session** | A connection that skipped the handshake; packets are unencrypted at the AdaTP layer (TLS may still protect the transport). |
| **Driver** | The server's credential-verification backend: `file`, `api`, or `none`. |
| **Tool** | A named server-side capability exposed by a plugin and invoked with `ToolCall`. |

## 4. Requirements language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in the
documents of this specification are to be interpreted as described in
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

Text not using these keywords, and all sections explicitly marked
*non-normative*, is informative.

## 5. Protocol version

This specification describes **protocol version 1**: the value carried in
the header `version` byte. Version-compatibility policy is defined in
[10-versioning.md](10-versioning.md).

## 6. Document map

| Document | Contents |
| :-- | :-- |
| [00-overview.md](00-overview.md) | This document. |
| [01-requirements.md](01-requirements.md) | What the protocol is required to provide. |
| [02-architecture.md](02-architecture.md) | Client/server model, rooms, planes, session lifecycle. |
| [03-framing.md](03-framing.md) | Byte-exact framing and WebSocket mapping. |
| [04-packets.md](04-packets.md) | Message-type registry and payload schemas. |
| [05-state-machines.md](05-state-machines.md) | Connection state machine and counters. |
| [06-signaling.md](06-signaling.md) | Text-based signaling grammar. |
| [07-media-game.md](07-media-game.md) | Audio format, voice routing, GameState. |
| [08-security.md](08-security.md) | Threat model, cryptography, limitations. |
| [09-extensions.md](09-extensions.md) | Tools, custom events, reserved ranges. |
| [10-versioning.md](10-versioning.md) | Version byte and compatibility policy. |
| [11-conformance.md](11-conformance.md) | Conformance levels and test procedure. |
| [12-authenticated-handshake.md](12-authenticated-handshake.md) | **Proposed v2:** authenticated key exchange (design + [formal model](formal/); not yet implemented). |
| [appendix-error-codes.md](appendix-error-codes.md) | Error and close-reason registry. |
| [appendix-test-vectors.md](appendix-test-vectors.md) | Golden test vectors. |

Related non-spec documents: the narrative cryptography guide
(`../protocol/crypto.md`), the reliability model
(`../architecture/reliability.md`), the port reference
(`../deployment/ports.md`), and the raw-TCP retirement note
(`../legacy.md`).
