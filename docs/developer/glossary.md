# Glossary

**Packet** — one AdaTP unit: 45-byte header + payload (+ 16-byte auth tag
when encrypted). Exactly one packet per WebSocket binary message.

**Frame / framing** — the byte layout of a packet on the wire
([spec §3](../spec/03-framing.md)). "Frame" and "packet" are used
interchangeably in this project.

**Magic** — the constant `0x41444154` ("ADAT") opening every packet.

**Session id** — the 16-byte, client-chosen identity in every header;
pinned by the server to a connection's first packet and stamped on all
routed traffic as the sender address.

**Room** — the routing scope. Each authenticated connection is in exactly
one room (default `global`); routable packets broadcast to all members,
sender included.

**Hub** — the server's in-memory connection/room registry and router.

**Secure session** — a connection upgraded via the X25519 handshake:
HKDF-SHA256-derived keys, AES-256-GCM per packet, per-direction sequence
counters. Client↔server transport encryption (not end-to-end).

**Plaintext session** — no handshake, payloads as-is (the browser SDK);
protected by `wss://` in production.

**Auth driver** — the server's credential verifier: `file` (users.json,
dev), `api` (your HTTP backend), `none` (anonymous dev mode).

**Plugin** — a separate OS process managed by the server, speaking NDJSON
over stdio, declared by a `plugin.json` manifest with default-deny
permissions. [Guide](../platform/PLUGIN_DEVELOPMENT.md).

**Hook** — a plugin subscription to server moments (`auth`, `text`,
`file`, `join`/`leave`, `presence`, `tool_before`/`tool_after`,
`shutdown`). Veto hooks may block the action; notify hooks just observe.

**Tool** — a named, schema-described function a plugin exposes; clients
call it with `ToolCall (0x0070)` and a correlation id.
`system.list_tools` is the built-in directory.

**GameState** — packet type `0x0050`: opaque room-routed state snapshots,
JSON recommended. [Deep dive](game-state.md).

**Webhook endpoint** — an HTTPS URL registered with the server that
receives HMAC-SHA256-signed JSON deliveries for subscribed events.

**Delivery** — one webhook POST attempt-group (unique `X-AdaTP-Delivery`
id, up to 5 attempts, circuit breaker per endpoint).

**Event** — a named occurrence on the bus (`auth.success`,
`room.joined`, `plugin.<name>.<custom>`, …) fanned out to webhooks.

**Drain** — operator mode where `/readyz` reports 503 and new WebSocket
connections are refused while existing ones continue; the load balancer's
signal to route elsewhere.

**LB hints** — `GET /admin/v1/lb-hints`: healthy/draining flags plus
connection counts for load-balancer decisions.

**Silo Panel** — the SCADA-style operator UI embedded in the server
binary at `/silo`, driven entirely by the admin API.

**Admin token** — the bearer credential for `/admin/v1` (env
`ADMIN_TOKEN`, or generated and logged at boot).

**Golden vectors** — the deterministic byte-exact test cases
(`tests/conformance/vectors/…`) that every implementation must reproduce;
the spec's executable half.

**Conformance level** — how much of the protocol an implementation
claims: Core (plaintext framing + auth + rooms), Secure (+ the encrypted
session), Tools (+ tool packets). Defined in
[spec §11](../spec/11-conformance.md).

**Signaling grammar** — the reserved `TextMessage` vocabulary
(`INVITE:…`, `DISCOVERY:…`, `MUTE:…`, `SYS:PING`, `TOOL:`/`TOOLRESULT:`)
— [spec §6](../spec/06-signaling.md).

**At-most-once** — AdaTP's delivery promise: a routed packet arrives zero
or one times; nothing is stored or retransmitted by the server.
