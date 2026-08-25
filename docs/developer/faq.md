# FAQ

**Port 3000 is already in use — what now?**
Something else (often a Vite/Next dev server) owns it. Run AdaTP on
another port: `PORT=3100 ./adatp-server`, and point clients at
`ws://127.0.0.1:3100/ws`. Find the squatter with
`lsof -nP -iTCP:3000 -sTCP:LISTEN`. The integration tests pick a free
port automatically. See [`docs/deployment/ports.md`](../deployment/ports.md).

**Is AdaTP encryption end-to-end?**
No, and the docs never claim it. The X25519/AES-GCM session encrypts
**client↔server**; the server decrypts to route and re-encrypts per
recipient. The handshake is unauthenticated, so its MITM protection comes
from TLS — production runs behind `wss://`. Honest threat model:
[`docs/spec/08-security.md`](../spec/08-security.md).

**Why doesn't the browser SDK do the secure handshake?**
Browser pages already get transport security from `wss://`, and keeping
the browser client plaintext keeps it dependency-free. The native SDKs
(Node/Python/PHP/C/Arduino) all do the in-protocol handshake.

**Are messages persisted? Can a client fetch history?**
No. Delivery is at-most-once to *current* room members; the server stores
nothing. History, inboxes and offline delivery are application concerns
(a plugin + webhook consumer is a good place to build them).

**What's the maximum message size?**
`MAX_FRAME_BYTES` (default 1 MiB of payload). Larger → connection closed
with `frame_too_large`. Files are chunked (16 KiB) so they never hit it.

**Are there room limits? How do I create/delete rooms?**
You don't — rooms exist while occupied. Names: 1–128 chars, no control
characters. There's no per-room member cap in v1; capacity planning is
bandwidth math ([voice](voice.md), [reliability](../architecture/reliability.md)).

**How do I run without users.json?**
`AUTH_DRIVER=none` accepts any credentials with role `anonymous` —
development only. The file driver's plaintext `users.json` is also
dev/demo-grade.

**What should production authentication look like?**
`AUTH_DRIVER=api`: the server POSTs `{username,password}` to your
`AUTH_API_URL` and honors `{authorized,user_id,role}`. Your backend owns
credential storage/hashing. If it's down, AdaTP **fails closed**.
Details: [production portal](../production/README.md).

**How do I see who's online?**
In-protocol: track `PresenceUpdate` JOIN/LEAVE in your room. Out-of-band:
`GET /admin/v1/connections` or the [Silo Panel](../platform/silo-panel.md)
CONNECTIONS tab.

**Does AdaTP scale horizontally?**
v1 is single-node (in-memory rooms). For multiple nodes, shard rooms
across servers at your load balancer and use `GET /admin/v1/lb-hints`
(healthy / draining / capacity) for routing decisions. Cross-node room
federation is not in v1 — stated honestly in
[reliability](../architecture/reliability.md).

**Self-signed TLS in front of the server?**
Fine for testing: browsers must first trust the cert (visit the https URL
once); Node `ws` needs `NODE_TLS_REJECT_UNAUTHORIZED=0` (never in prod);
Python `websocket-client` accepts `sslopt={"cert_reqs": ssl.CERT_NONE}`.
Production: real certificates at the proxy
([tls guide](../production/README.md)).

**Why does my client receive its own messages?**
By design — room broadcasts include the sender. It confirms delivery and
powers RTT measurement (`SYS:PING`). Filter by comparing sender session id
with `getMyId()` / your own id.

**My JoinRoom reply seems to be a presence packet…**
The stream is multiplexed; broadcasts interleave with replies. Wait *by
type* (`readNextPacketOfType`, `readPacketOfType`) — the SDK helpers
already do.

**Can I run plugins written in Python/Go/anything?**
Yes — a plugin is any executable speaking NDJSON on stdio; the manifest's
`entry` is an argv array. See
[PLUGIN_DEVELOPMENT.md](../platform/PLUGIN_DEVELOPMENT.md).

**Why is my webhook never delivered to http://localhost:…?**
The SSRF guard blocks private/loopback targets by default. For local
development start the server with `ADATP_WEBHOOK_ALLOW_PRIVATE=1`.

**Where are the protocol test vectors?**
`tests/conformance/vectors/adatp-v1-vectors.json`, embedded in
[`docs/spec/appendix-test-vectors.md`](../spec/appendix-test-vectors.md),
replayed by Rust, Node and Python runners
([testing](../testing/README.md)).

**Is there message compression?**
The `COMPRESSED` flag bit is reserved but has no v1 semantics — nothing
sets it. WebSocket permessage-deflate is likewise not negotiated by the
reference server.
