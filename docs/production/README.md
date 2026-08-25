# AdaTP Production Portal

Operator documentation for running AdaTP v1 in production.

**Read this first:** AdaTP v1 is a **single-node** realtime server. Rooms and
connections live in memory; only API keys and webhook endpoints persist (SQLite).
Plan capacity, HA and backups with that model in mind — the pages below never
promise clustering or message persistence, because v1 does not have them.

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
