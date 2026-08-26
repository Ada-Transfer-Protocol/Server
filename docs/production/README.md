# AdaTP Production Portal

Operator documentation for running AdaTP v1 in production.

**Read this first:** AdaTP v1 is a **single-node** realtime server. Rooms and
connections live in memory; only API keys and webhook endpoints persist (SQLite).
Plan capacity, HA and backups with that model in mind — the pages below never
promise clustering or message persistence, because v1 does not have them.

The security model that governs every page here is
[`../SECURITY_MODEL.md`](../SECURITY_MODEL.md) — in short: **run behind TLS**,
because AdaTP's own crypto is hop-by-hop (not E2E) and its handshake is not yet
authenticated.

## Production readiness at a glance

What is actually **Shipped** in the server, what **you must provide** as the
operator, and what is **Roadmap** (does not exist in v1 — do not design around
it). Details are on each linked page; the roadmap column tracks
[`../../ROADMAP.md`](../../ROADMAP.md).

| Topic | Status | The honest one-liner |
| :-- | :-- | :-- |
| **TLS / edge** | Operator-provides · native listener is Roadmap | The server speaks plain `ws://`; you terminate `wss://` at a proxy. **Mandatory.** ([tls-cloudflare.md](./tls-cloudflare.md)) |
| **Authentication** | **Shipped** (`file`/`api`/`none`, fail-closed) · IdP Operator-provides | Real per-connection verification; use `api` against your identity system. ([auth-providers.md](./auth-providers.md)) |
| **Security hardening** | **Shipped** controls · deployment Operator-provides | Room isolation, attempt caps, constant-time token checks; you own network + secrets. ([security-hardening.md](./security-hardening.md)) |
| **Webhooks** | **Shipped** | HMAC-SHA256 signing, SSRF guard (v4+v6), retries, circuit breaker, audit log. ([webhooks-ops.md](./webhooks-ops.md)) |
| **Plugins** | **Shipped** | Process isolation, default-deny manifests, timeouts, rate limits. ([plugins-ops.md](./plugins-ops.md)) |
| **Silo operator panel** | **Shipped** | Embedded SCADA-style UI at `/silo`. ([silo-panel-ops.md](./silo-panel-ops.md)) |
| **Observability** | **Shipped** (metrics, logs, load series) · scrape/alert backend Operator-provides | Endpoints and an SSE log stream exist; wiring to your stack is yours. ([observability.md](./observability.md)) |
| **Sizing** | **Shipped** harness · figures are **guidance, not SLAs** | Cost is fan-out, not connection count; verify on your hardware. ([sizing.md](./sizing.md)) |
| **Performance tuning** | **Shipped** (a short, real knob list) | Biggest levers are build type, room shape, OS limits — not envs. ([performance-tuning.md](./performance-tuning.md)) |
| **Incident runbook** | **Shipped** (commands map to real endpoints) | Drain, kick, restart, fail-closed rehearsal. ([incident-runbook.md](./incident-runbook.md)) |
| **Backup / restore** | **Shipped** (SQLite is a file) · schedule/offsite Operator-provides | Only API keys + webhook config persist; messages never do. ([backup.md](./backup.md)) |
| **Upgrade / rollback** | **Shipped** (drain-based) · artifact mgmt Operator-provides | Every upgrade is a restart; drain turns it into an announced blip. ([upgrade-rollback.md](./upgrade-rollback.md)) |
| **Kubernetes** | **Shipped** single-replica starter · autoscaling Operator-provides | One replica per instance — there is nothing to cluster in v1. ([install-kubernetes.md](./install-kubernetes.md)) |
| **High availability** | Fast *recovery* Operator-builds · zero-downtime *failover* is **Roadmap** | Supervised restart + active/passive; no shared state between nodes. ([ha.md](./ha.md)) |
| **Clustering / multi-node rooms** | **Roadmap** (state backplane) | Two servers are two separate worlds today. ([ha.md](./ha.md)) |
| **Message persistence / replay** | **Roadmap** | Delivery is at-most-once, by design in v1. ([../architecture/reliability.md](../architecture/reliability.md)) |

## Recommended reading order

### 1. Install
| Page | What it covers |
| :-- | :-- |
| [install-binary.md](./install-binary.md) | Bare-metal / VM install with systemd |
| [install-docker.md](./install-docker.md) | Docker image + compose deployment |
| [install-kubernetes.md](./install-kubernetes.md) | Single-replica Kubernetes starter |

### 2. Configure
| Page | What it covers |
| :-- | :-- |
| [configuration-reference.md](./configuration-reference.md) | Every environment variable |
| [auth-providers.md](./auth-providers.md) | file / api / none drivers, backend contract |
| [tls-cloudflare.md](./tls-cloudflare.md) | TLS termination: Cloudflare, nginx, caddy |

### 3. Secure
| Page | What it covers |
| :-- | :-- |
| [security-hardening.md](./security-hardening.md) | Hardening checklist (tokens, keys, network) |
| [webhooks-ops.md](./webhooks-ops.md) | Webhook secrets, SSRF policy, breaker ops |
| [plugins-ops.md](./plugins-ops.md) | Plugin deployment, isolation, supply chain |

### 4. Observe
| Page | What it covers |
| :-- | :-- |
| [observability.md](./observability.md) | Metrics, logs, load series — and honest gaps |
| [silo-panel-ops.md](./silo-panel-ops.md) | Operating the Silo control panel |

### 5. Operate
| Page | What it covers |
| :-- | :-- |
| [architecture.md](./architecture.md) | Production topology and failure domains |
| [sizing.md](./sizing.md) | Capacity drivers and load-test methodology |
| [ha.md](./ha.md) | What HA is (and is not) possible in v1 |
| [backup.md](./backup.md) | What to back up and how to restore |
| [incident-runbook.md](./incident-runbook.md) | Failure scenarios with exact commands |
| [upgrade-rollback.md](./upgrade-rollback.md) | Drain-based upgrades and rollback |
| [performance-tuning.md](./performance-tuning.md) | The knobs that actually exist |
| [checklist-go-live.md](./checklist-go-live.md) | Final gate before real traffic |

## Companion documentation

- Protocol: [`SPEC.md`](../SPEC.md) and [`docs/spec/`](../spec/00-overview.md)
- Security model: [`docs/spec/08-security.md`](../spec/08-security.md)
- Reliability model: [`docs/architecture/reliability.md`](../architecture/reliability.md)
- Platform (plugins / webhooks / admin API / Silo): [`docs/platform/`](../platform/README.md)
- Ports: [`docs/deployment/ports.md`](../deployment/ports.md) ·
  Quickstart: [`docs/deployment/quickstart.md`](../deployment/quickstart.md)
- Testing: [`docs/testing/README.md`](../testing/README.md)
