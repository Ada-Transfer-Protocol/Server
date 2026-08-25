# AdaTP Platform Documentation

The platform layer is everything around the data plane: extensibility
(plugins and their tools), outbound integration (signed webhooks), and the
operator control plane (admin API + the embedded Silo panel). Plugins run
as fault-isolated child processes with default-deny permissions; webhooks
deliver signed events asynchronously so integrations never touch data-plane
latency; and the Silo panel is a SCADA-style UI over the same `/admin/v1`
API that scripts and orchestrators use — one control surface, two skins.

| Guide | What it covers |
| :-- | :-- |
| [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) | Building plugins: manifest, permissions, NDJSON protocol, tools, hooks, lifecycle, the bundled echo/moderation examples. |
| [WEBHOOK_DEVELOPMENT.md](WEBHOOK_DEVELOPMENT.md) | Consuming webhooks: event catalog, HMAC-SHA256 verification (Node + Python), retries/breaker/SSRF, receiver checklist, ops. |
| [admin-api.md](admin-api.md) | The `/admin/v1` reference: auth model, every endpoint with examples. |
| [silo-panel.md](silo-panel.md) | Operating the embedded Silo panel and proving it renders live state. |

Agent-focused material (agents as clients, as tools, orchestration):
[../developer/AI_AGENT_DEVELOPMENT.md](../developer/AI_AGENT_DEVELOPMENT.md).
Protocol-level extension rules (tool packets, reserved ranges):
[../spec/09-extensions.md](../spec/09-extensions.md).
