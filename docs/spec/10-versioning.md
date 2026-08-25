# AdaTP Specification — 10: Versioning

**Status:** Normative, v1.0

---

## 1. The version byte

Every packet header carries a one-byte protocol version at offset 4
([03-framing.md](03-framing.md)). This specification defines
**version 1**. The version byte identifies the *wire protocol*, not the
software release: server builds, SDK releases, and this document set
version independently (the platform ships as release v1.0.0 speaking
protocol version 1).

## 2. Rules for version 1 endpoints

- **V-1** Senders MUST set `version = 1`.
- **V-2** A receiver encountering a packet whose `version` it does not
  implement MUST close the connection. There is no downgrade path inside
  a connection.
- **V-3** The version is per-**packet** on the wire but MUST be constant
  per connection; a mid-stream version change is a protocol violation
  (close, `malformed_packet`).

## 3. What requires a version bump

The version byte MUST be incremented for any change that would make a
correct v1 implementation misinterpret bytes or violate v1 guarantees:

- header layout changes (field sizes, order, endianness, new mandatory
  fields);
- semantic changes to existing message types, flags, or fields
  (e.g. activating `COMPRESSED`/`RELIABLE`, changing nonce derivation);
- changes to the handshake or key-derivation procedure;
- removing or repurposing an **Active** code point.

The following do **not** bump the version (backward-compatible by the
ignore rules [E-1/E-2/E-4](09-extensions.md)):

- assigning new message types in reserved/unassigned ranges;
- activating a type previously documented as Reserved, with its documented
  semantics;
- adding optional JSON members to existing payloads;
- server-side policy changes (limits, timeouts, auth drivers).

## 4. Negotiation — honest status

**Version 1 has no version-negotiation exchange.** There is no
"supported versions" packet; a client discovers incompatibility only by
being disconnected (V-2). Consequences:

- A v1 client connecting to a hypothetical v2-only server will be closed
  on its first packet; SDKs SHOULD surface this as a clear
  "protocol version rejected" error rather than a generic disconnect.
- Servers introducing v2 SHOULD accept v1 *and* v2 during a transition
  window (per-connection, fixed at the first packet), since the header
  location of the version byte is stable.
- A negotiation packet (client advertises supported versions inside
  `HandshakeInit` or a new type) is the expected v2 mechanism; it is
  deliberately **not** retrofitted into v1 documentation as if it
  existed.

## 5. Document and vector versioning

- The specification documents in `docs/spec/` describe exactly one wire
  version; a future v2 forks the directory rather than annotating this
  one.
- The golden vector file
  (`tests/conformance/vectors/adatp-v1-vectors.json`) carries its own
  `version` field; vectors are append-only within a wire version —
  changing an existing vector's expected bytes is by definition a wire
  change and forbidden without a version bump.

## 6. Deprecation policy

- Code points can move between **Active** and **Reserved** status only at
  a wire-version boundary (the v1.0 renumbering of GameState, tools,
  Ping/Pong and video documented in [04-packets.md §1](04-packets.md)
  happened before any interoperating release existed).
- Within v1's lifetime, deprecations are documentation-level ("SHOULD NOT
  send") and never change receiver behaviour.
