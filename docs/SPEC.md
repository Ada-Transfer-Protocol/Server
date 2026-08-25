# AdaTP — Specification Index

**Ada Transfer Protocol, wire version 1** — a lightweight binary realtime
protocol over WebSocket for text, files, voice, shared game state, and
tool invocation.

The normative text lives under [`docs/spec/`](spec/). This page is
the index. Requirements language follows RFC 2119
([overview §4](spec/00-overview.md)).

## Normative documents

| # | Document | Status |
| :-- | :-- | :-- |
| 00 | [Overview](spec/00-overview.md) | Normative v1.0 |
| 01 | [Requirements](spec/01-requirements.md) | Normative v1.0 |
| 02 | [Architecture](spec/02-architecture.md) | Normative v1.0 |
| 03 | [Framing](spec/03-framing.md) | Normative v1.0 |
| 04 | [Packets](spec/04-packets.md) | Normative v1.0 |
| 05 | [State Machines](spec/05-state-machines.md) | Normative v1.0 |
| 06 | [Signaling](spec/06-signaling.md) | Normative v1.0 |
| 07 | [Media & Game State](spec/07-media-game.md) | Normative v1.0 |
| 08 | [Security](spec/08-security.md) | Normative v1.0 |
| 09 | [Extensions](spec/09-extensions.md) | Normative v1.0 |
| 10 | [Versioning](spec/10-versioning.md) | Normative v1.0 |
| 11 | [Conformance](spec/11-conformance.md) | Normative v1.0 |
| A | [Appendix: Error Codes](spec/appendix-error-codes.md) | Normative v1.0 |
| B | [Appendix: Test Vectors](spec/appendix-test-vectors.md) | Normative v1.0 (source of truth: [`tests/conformance/vectors/adatp-v1-vectors.json`](../tests/conformance/vectors/adatp-v1-vectors.json)) |

## The one-paragraph version

Clients open `ws(s)://host:3000/ws` and exchange 45-byte-headered binary
packets — one per WebSocket message. A connection optionally upgrades to
an AES-256-GCM channel via an X25519 handshake, authenticates with real
credentials (fail-closed), lands in room `global`, and from there joins
rooms and exchanges text, files, PCM voice, game state, and tool calls,
which the server routes to the room (sender included) with explicit,
registered error codes for every refusal.

## Companion documents (informative)

- [Cryptography guide](protocol/crypto.md) — narrative walkthrough of
  the security layer and its honest limits.
- [Reliability model](architecture/reliability.md) — delivery
  semantics, limits, backpressure, non-claims.
- [Port reference](deployment/ports.md) — canonical 3000/443, retired
  ports.
- [Legacy: raw-TCP listener](legacy.md) — the pre-1.0 `:8444`
  carrier and its removal.
- [Compact wire reference](PROTOCOL_SPEC.md) — single-page
  summary maintained alongside the server.

## Verifying an implementation

```bash
bash tests/integration/run.sh   # live Core+Secure suite (auto-port)
# golden vectors: tests/conformance/vectors/adatp-v1-vectors.json
```

See [Conformance](spec/11-conformance.md) for levels (Core / Secure /
Tools) and the claim procedure.
