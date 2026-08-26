# AdaTP — Roadmap & Maturity Ladder

This roadmap is deliberately honest. A professional review made one point that
governs everything here: **credibility comes from verified work and independent
adoption, not from documentation.** A protocol is trusted when other people bet
their products on it — that is earned in this order, and it cannot be skipped.

## Positioning (the axis we own)

AdaTP is **not** trying to be a general-purpose realtime bus that beats
Centrifugo/NATS on raw latency. That race has incumbents and no prize for
second place. AdaTP's defensible, mostly-empty axis is:

> **The only realtime protocol where a ~20 KB-RAM MCU (e.g. an STM32) is a
> first-class peer with the browser — the same rooms, the same crypto, the same
> conformance vectors, on the microcontroller and in Chrome.**

Target domains: **IoT / embedded, industrial & factory automation, high-speed
manufacturing telemetry, robotics and UAVs/drones.** In those places the
question "why AdaTP?" has a one-sentence answer: *"because your device has 20 KB
of RAM and nothing else fits there — while the same protocol still talks to a
browser tab."* MQTT has no rooms/voice/files; WebRTC won't fit an MCU; LiveKit
and Matrix are server-heavy. The **C / embedded SDK is therefore the crown
jewel**, not a sixth-place afterthought.

## The ladder (where we are, honestly)

| Tier | Meaning | Status |
| :-- | :-- | :-- |
| 6 → 7 | Works, decently written | **Reached.** Clean auth, process-isolated plugins, signed webhooks, graceful drain, health/readiness. |
| 8 | A serious alternative | **In progress** (this roadmap's near-term). |
| 9 | Audited & independently validated | Roadmap. Needs money + outside parties. |
| 10 | Infrastructure others build on | Multi-year, multi-person, capital. Not a side-project outcome. |
| 11 | Own a unique axis | The embedded-first thesis above — reachable, and the actual goal. |

10/10 is **not** a coding feature — WireGuard's crypto is ~4 000 lines and NATS
isn't much bigger than this server; what makes them 10s is verified trust and
adoption. Chasing a 10/10 "badge" is the wrong target. **Being a 9 in the narrow
"realtime hub for constrained devices" category is worth more than a 6 in the
general-infrastructure category** — less work, more defensible, and it maps
directly onto the products this protocol was built to serve.

---

## Done in this cycle (verified, not aspirational)

The review's highest-credibility items — the ones that stop the docs from
over-promising — were executed and verified:

- **Radical doc honesty.** Removed over-claims (E2E → hop-by-hop; corrected
  transport/ACID/uptime statements); added a thorough `SECURITY_MODEL.md`;
  marked every production doc **Shipped / Operator-provides / Roadmap**.
- **Security hardening (code, tested).** Real replay rejection; room-join
  authorization (plugin veto + policy); connection & message rate limits +
  resource caps; encryption-downgrade rejection once a session exists.
- **Repo hygiene.** Removed committed demo credentials, added a safe
  unconfigured-startup default, `.gitignore` cleanup.
- **Real supporting artifacts** so ops docs describe reality: `cargo-fuzz`
  target for the parser, GitHub Actions CI (build + conformance + fmt/clippy +
  fuzz smoke), applyable Kubernetes manifests, a TLS-terminating docker-compose,
  a reproducible benchmark harness (with an **empty** results table — numbers
  are measured, never invented).
- **Cloud** gained an **automatic status/uptime monitor** derived from real
  fleet heartbeats, and its copy was corrected to the honest crypto model.
- Published the **Plugin-SDK** repo (manifest + NDJSON protocol reference,
  examples, template, local harness).

---

## Tier 8 — "a serious alternative" (single-maintainer, ~6–12 months)

1. **Backplane / multi-node.** ✅ **Landed (first cut).** A `Backplane` seam with
   a single-node default and a **Redis pub/sub** implementation for cross-node
   room fan-out (`ADATP_BACKPLANE_URL`, `server/src/backplane.rs`), verified with
   a two-node cross-node test ([`tests/backplane/`](tests/backplane/)). `ha.md`
   now describes real, tested behaviour. **Remaining:** cross-node presence/
   membership, and stronger delivery than best-effort (Redis Streams / NATS
   JetStream) for at-least-once.
2. **Published benchmarks.** 🟡 **Reference sample published** — real p50/p95/p99,
   throughput, CPU + RSS at 100/250/500 clients from the harness, in
   [`docs/production/benchmarks.md`](docs/production/benchmarks.md) (debug build,
   laptop — honest sample, not a spec). **Remaining:** a `--release`, 10k/50k
   run on dedicated hardware, and the headline number on the README.
3. **Continuous fuzzing.** The `cargo-fuzz` target running in CI on every push;
   a binary parser that isn't fuzzed is not acceptable.
4. **Real observability.** OpenTelemetry traces + a proper Prometheus metric set
   (the current `/metrics` is a counter, not telemetry) + structured logging.
5. **Semver + LTS + upgrade path.** A breaking-change policy and a tested
   rolling-upgrade procedure.

## Tier 9 — "audited" (needs external parties + capital)

- **Independent security audit** (Trail of Bits / Cure53 / NCC class,
  ~$40–80k, 4–8 weeks), report published, findings closed. For a product that
  makes a crypto claim there is no substitute.
- **Formal verification** of the handshake (Tamarin/ProVerif) — the difference
  between *saying* "MITM-resistant" and *proving* it. **Free**, and TLS 1.3 /
  Signal / WireGuard all did it.
- **A second, independent implementation** written only from `docs/spec/` that
  passes the conformance vectors — the only real test that the spec is a spec.
- **Named reference deployments** running 12+ months in production.

## The crypto decision (blocks the security-claim tier)

The app-layer handshake is currently **unauthenticated** (no Ed25519 signing of
the X25519 exchange), so it is MITM-able without TLS. Today's honest posture:
**TLS at the edge is mandatory** and provides server authentication +
confidentiality; AdaTP's own encryption is defense-in-depth, hop-by-hop, not
E2E. Two ways forward — pick one deliberately:

- **(A) Authenticate the AKE:** add an Ed25519 server identity + signed
  transcript (and pinned/TOFU server keys in the SDKs), make encryption
  mandatory, authenticate the header as AEAD AAD, enforce the replay window.
  This is a **breaking change across all six SDKs + the spec + the conformance
  vectors** and should be done as one coordinated, verified release — not
  piecemeal.
- **(B) Delegate to TLS:** treat TLS as the security boundary, keep app-crypto
  as optional defense-in-depth, and stop making standalone-secure-channel
  claims. Less code, less risk, and it matches how the system is actually run.

The review recommends (B); (A) is the path if "secure protocol" must stand on
its own without TLS. Either is honest — the current in-between is not.

**Decision: (A), because the embedded-first bet requires it.** A ~20 KB MCU
often can't run TLS but can run X25519/Ed25519 — under (B) that peer has no
security without TLS, which removes the one axis this project owns. But (A)
ships **through verification, not before it** — a hand-rolled AKE announced as
"MITM-resistant" would be the very docs-ahead-of-code failure this review is
about. Sequence:

1. **Specify** — done: [`docs/spec/12-authenticated-handshake.md`](docs/spec/12-authenticated-handshake.md)
   (SIGMA-style Ed25519-signed transcript, key pinning/TOFU, downgrade defense,
   header-as-AAD), as an **opt-in protocol v2** — v1 untouched.
2. **Formally model** — **done and run** ([`docs/spec/formal/`](docs/spec/formal/),
   [`RESULTS.md`](docs/spec/formal/RESULTS.md)): ProVerif confirms secrecy + no
   MITM for v2 and reconstructs the MITM for v1. The free half of the tier-9
   "prove it" step — cleared.
3. **Verify** — the **mixed-version downgrade query is now modeled and passes**
   (`docs/spec/formal/adatp_v2_downgrade.pv`: a pinning client completes only via
   v2). Remaining: expert review + an independent audit (the paid tier-9 item).
   Symbolic ≠ audited.
4. **Implement** — **server + five SDKs done, verified end-to-end**:
   `handshake_v2.rs` + `connection.rs` negotiate v2 on `version>=2` (persistent
   Ed25519 identity, v1 untouched); the **header is bound as AEAD AAD** in v2;
   `ADATP_MIN_PROTOCOL_VERSION=2` enforces the authenticated handshake. Five
   independent clients now implement v2 (pin + signature verify + Finished +
   AAD) and each passes a live e2e handshake + AAD round-trip against this
   server, plus golden-vector conformance in CI:
   **Node, C, Python, PHP** (and the Rust reference). Vectors are machine-checked
   in every language.
   **Remaining SDKs, with honest reasons:**
   - **browser-JS** (`js/`): plaintext-over-`wss` by design — it has no session
     crypto and delegates confidentiality/authentication to TLS. v2 targets peers
     that *cannot* run TLS; a browser always can, so v2 is intentionally not
     implemented there (not a gap).
   - **Arduino/ESP32**: has X25519+GCM via mbedTLS, but stock ESP32 mbedTLS ships
     **no EdDSA**, so Ed25519 signature verification needs an added
     implementation; and it cannot be built/hardware-tested in this environment.
     Deferred rather than shipped unverified.
5. Update the security claims **last** — after the fleet is complete and an audit
   lands.

Five interoperating implementations speak v2 end-to-end (server + Node + C +
Python + PHP), formally checked and conformance-tested. But TLS remains the
**blanket** recommendation until the story is complete per-deployment (the client
must speak v2 + pin + set the floor) and an independent audit is done.

## Tier 11 — the embedded-first bet (the actual goal)

1. **Crypto that runs and is measured on the MCU.** X25519 handshake latency
   and RAM on an STM32F103 — the number goes at the **top** of the README.
2. **Conformance suite on the embedded target** (QEMU or a real board in CI):
   *"the same vectors pass on the MCU."* Nobody else does this.
3. **Narrow the positioning** to "realtime messaging for constrained devices."
   Stop competing as general infrastructure; win the one race that's empty.

A project that can answer *"why use this?"* with *"because your device is 20 KB
and nothing else fits"* never needs a 10/10 badge.

---

## Realtime-platform track (broadcast transport / Reverb-parity)

Making AdaTP usable as a drop-in realtime backbone (app-server publish, Laravel
Reverb parity, cluster scale). Kept honest: shipped vs planned, separated.

**Shipped + verified:**
- **HTTP publish endpoint** — `POST /publish`, HMAC-SHA256 signed over
  `<ts>.<body>` with a replay window, single or batch, per-room delivery counts;
  `server/src/publish.rs`. An app server fans out to rooms without a socket.
- **Sender exclusion** (`->toOthers()`) — `hub::broadcast_publish(exclude_session)`,
  honoured cluster-wide (carried in the backplane envelope).
- **Multi-node backplane** — Redis pub/sub, active-active rooms, cross-node
  verified (`tests/backplane/`). Manageable from the Cloud admin + customer panels.
- **`auth_string`** — single-credential auth across the server (none / file-token /
  webhook) and the Node/C/Python/PHP SDKs, verified e2e.
- **Per-channel authorization** — a `private-`/`presence-` room requires a grant
  the app server signs (HMAC-SHA256 over the exact payload: room + session id +
  expiry [+ presence identity]); the client presents it on `JoinRoom` as
  `{room, grant}` and the hub verifies signature, expiry, room and session before
  admitting the join (`server/src/channel_auth.rs`). Verified e2e: public joins
  free, private rejected without a grant, joined with a valid one.
- **Presence** — `presence-*` rooms keep a member roster (deduped by `user_id`,
  identity taken only from the signed grant). The joiner receives
  `presence:here`; the room is told `presence:member_added` on a user's first
  session and `presence:member_removed` when their last session leaves
  (`hub::presence_join`/`presence_leave`). Verified e2e with two clients.
  **Node-local** for now — cross-node presence is still ahead (see below).
- **Client events** (whispers) — `client.whisper()` relays a `client-*` event to
  the *other* members of a private/presence room, never the sender, never
  persisted; dropped on public rooms and for non-`client-*` names
  (`MessageType::ClientEvent`). Verified e2e (delivery, no self-echo, public drop).

**Planned (named, not built):**
- **Cross-node presence** — the roster is currently per-node; making
  `member_added`/`removed` and the `here` roster span the fleet needs the
  backplane to carry presence deltas + a shared roster store.
- **`adatp-laravel`** broadcast driver + device-fleet client; **`adatp-echo`**
  Laravel Echo connector + a parity suite against Reverb.
- **Cloud**: per-project auth "middleware" config pushed to nodes; admin
  "log in as customer"; **Caddy** reverse proxy with per-node subdomains
  (`[prefix]-customer-node` / `-official`), automatic domain + SSL, and an
  "AdaTP is actually installed" check before wiring a node.
- **Backplane adapters** beyond Redis (NATS / others) selectable per deployment.
- **Connection-state recovery** — Redis-Streams offsets + `JoinRoom since_offset`
  so a reconnecting client replays what it missed (the honest wording is
  "message recovery on reconnect", not "seamless failover").
- **Callback ack + timeout** (configurable) and an **HTTP long-polling fallback**
  when WebSocket is unavailable.

The single-node channel semantics (auth, presence, client events) now match
Reverb; a migration guide must still say plainly where AdaTP is behind —
**cross-node presence**, **mature reconnect/state-recovery**, and the
**`adatp-laravel`/`adatp-echo`** packages themselves — hiding that is the kind
of claim this document exists to forbid.

---

*This document describes what is shipped, what is planned, and what would take
outside resources — and keeps those three clearly separated. That separation is
the point.*
