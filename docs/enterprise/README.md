# AdaTP — Honest Enterprise Readiness Checklist (v1.0.0)

A candid inventory for teams evaluating AdaTP for serious deployments.
"✅" means implemented and verified by the test suites; "⚠️" means partial
with documented caveats; "❌" means not in v1 (roadmap).

## Protocol & correctness

| Capability | Status | Notes |
| :-- | :-- | :-- |
| Normative specification | ✅ | RFC-style pack in `docs/spec/`, RFC 2119 language |
| Golden test vectors | ✅ | 9 deterministic cases; replayed by Rust, Node, Python |
| Cross-SDK interop | ✅ | 6 SDKs on one wire format; encrypted sessions verified live |
| Versioning policy | ⚠️ | Version byte + documented rules; no negotiation packet in v1 |

## Security

| Capability | Status | Notes |
| :-- | :-- | :-- |
| Credential verification (fail-closed) | ✅ | file/api/none drivers; 3-strike lockout |
| Transport encryption | ⚠️ | AES-256-GCM sessions, but DH is unauthenticated → TLS (wss) is REQUIRED in production |
| End-to-end encryption | ❌ | Server routes plaintext by design (hop-by-hop) |
| SSO / OIDC / SAML | ❌ | Use `AUTH_DRIVER=api` to delegate to your IdP-backed endpoint |
| Secrets hygiene | ✅ | No secrets in git; admin token via env; webhook secrets shown once |
| Audit trail | ⚠️ | Admin actions logged + webhook delivery audit (in-memory, 256 entries); no persistent audit store |

## Operations

| Capability | Status | Notes |
| :-- | :-- | :-- |
| Health/readiness probes | ✅ | `/healthz`, `/readyz` (drain-aware) |
| Operator UI | ✅ | Silo Panel, live-bound to the admin API |
| Metrics | ⚠️ | JSON endpoints + 60 s load series; **no Prometheus/OTel exporter yet** |
| Structured log streaming | ✅ | Ring buffer + SSE (same stream as stderr) |
| Graceful drain | ✅ | LB-visible via `/readyz`; tested |
| Containerization | ✅ | Non-root image, healthcheck, compose; K8s manifests are single-replica |

## Scale & resilience

| Capability | Status | Notes |
| :-- | :-- | :-- |
| Single-node throughput | ✅ | Measured: ~4 k deliveries/s at p99 8 ms on a laptop debug build (see sizing.md) |
| Horizontal scaling / clustering | ❌ | Single node; rooms are in-memory; active-passive only |
| Message persistence / replay | ❌ | At-most-once, ephemeral by design |
| Backpressure policy | ✅ | Drop-and-count per slow consumer (documented, metered) |
| Rate limits | ⚠️ | Tool-level ✅; per-connection message-rate limits ❌ (edge concern) |

## Extensibility

| Capability | Status | Notes |
| :-- | :-- | :-- |
| Plugin platform | ✅ | Process-isolated, default-deny permissions, hooks + tools, metrics |
| Webhooks | ✅ | Signed, retried, circuit-broken, SSRF-guarded |
| Custom packet ranges | ✅ | Reserved ranges documented in `docs/spec/09-extensions.md` |

## Compliance posture

No certifications are claimed (SOC 2, ISO 27001, HIPAA: ❌). AdaTP is not
certified for safety-critical use (see `docs/architecture/reliability.md`,
"non-claims"). Evaluate the honest limitations in `SECURITY.md` against
your threat model before production use.
