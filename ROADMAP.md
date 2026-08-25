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

1. **Backplane / multi-node.** A `Backplane` seam with a single-node default and
   a Redis-Streams (or NATS) implementation for cross-node room fan-out. The day
   this lands, `ha.md` stops being fiction and horizontal scale is real.
2. **Published benchmarks.** p50/p95/p99 at 10k/50k concurrent connections, CPU,
   RAM, reconnect-storm behaviour — from the harness now in the repo, with the
   reproduce script. Numbers on the README's first screen.
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

*This document describes what is shipped, what is planned, and what would take
outside resources — and keeps those three clearly separated. That separation is
the point.*
