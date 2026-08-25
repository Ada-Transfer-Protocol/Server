# Go-Live Checklist

The final gate. Every row needs a checkmark, an owner, and evidence
(command output or screenshot) attached to your launch ticket.

> **Every gate below maps to a capability that actually ships in v1** — drain,
> kick, fail-closed auth, signed webhooks, the integration suite, etc. Nothing
> here depends on an unshipped feature; there is no "roadmap" tier to defer.
> The two things **you** must bring are on rows 1–2: **TLS** and a locked-down
> ops surface. Rationale for the TLS requirement:
> [`../SECURITY_MODEL.md`](../SECURITY_MODEL.md).

| # | Gate | How to verify | Ref |
| :-- | :-- | :-- | :-- |
| 1 | TLS end-to-end; origin port closed | `cargo run -p adatp-cli -- -a wss://<host>/ws -u probe -p …` succeeds; `curl -m3 http://<public-ip>:3000/healthz` fails | [tls-cloudflare.md](./tls-cloudflare.md) |
| 2 | Ops surfaces not public | `curl https://<host>/silo` and `/admin/v1/overview` → 403/blocked from the internet; reachable via VPN vhost | [tls-cloudflare.md](./tls-cloudflare.md) |
| 3 | `ADMIN_TOKEN` set from secret store | boot log has **no** "generated one for this run" line | [security-hardening.md](./security-hardening.md) |
| 4 | Bootstrap API key rotated | `adatp-admin auth list --db-url …` shows `admin-secret-key` inactive/absent; new key works on `/api/status` | [security-hardening.md](./security-hardening.md) |
| 5 | Production auth driver | `curl -H "x-admin-token: …" …/admin/v1/config` → `"auth_driver":"api"`; demo `users.json` absent from the host | [auth-providers.md](./auth-providers.md) |
| 6 | Auth backend fail-closed rehearsed | stop the IdP in staging → logins get `auth_unavailable`, existing sessions live | [incident-runbook.md](./incident-runbook.md) §2 |
| 7 | Webhooks verified | each endpoint: `POST …/webhooks/<id>/test` → consumer logs **signature VALID**; secrets in secret store | [webhooks-ops.md](./webhooks-ops.md) |
| 8 | SSRF guard active | boot log has **no** `ADATP_WEBHOOK_ALLOW_PRIVATE` warning | [webhooks-ops.md](./webhooks-ops.md) |
| 9 | Plugins reviewed & healthy | manifest permission review signed off; `…/admin/v1/plugins` all `running`, restarts=0 | [plugins-ops.md](./plugins-ops.md) |
| 10 | Load test at 2× expected peak | `tools/loadtest` on the **release** build: 0 connect failures, `dropped_messages` flat, p99 in budget | [sizing.md](./sizing.md) |
| 11 | Drain tested | engage → `/readyz` 503 + new conns refused; release → 200 | [upgrade-rollback.md](./upgrade-rollback.md) |
| 12 | Kick tested | connect a probe client, kick from Silo/API, it disconnects | [silo-panel-ops.md](./silo-panel-ops.md) |
| 13 | Backup + restore drilled | restore into a scratch host; webhooks/API keys present; test delivery OK | [backup.md](./backup.md) |
| 14 | Version pinned + rollback rehearsed | deploy uses immutable tag/binary; previous artifact staged; rollback executed once in staging | [upgrade-rollback.md](./upgrade-rollback.md) |
| 15 | Monitoring wired | external probe on `/healthz` via the edge; alerts on `readyz≠200`, `dropped_messages` delta, auth-failure rate | [observability.md](./observability.md) |
| 16 | Logs shipped + level right | `RUST_LOG=warn`; journald/docker logs flowing to your aggregator | [observability.md](./observability.md) |
| 17 | Runbook + on-call ready | on-call has `ADMIN_TOKEN` access path, incident-runbook bookmarked, Silo reachable | [incident-runbook.md](./incident-runbook.md) |
| 18 | Client reconnect behavior verified | kill the server in staging while app clients are live → they reconnect, re-auth, re-join | [ha.md](./ha.md) |
| 19 | Integration suite green on the release artifact | `bash tests/integration/run.sh` → 4× PASS | [../testing/README.md](../testing/README.md) |
| 20 | Limits sized | `LimitNOFILE`/ulimits ≥ 2× target connections; `MAX_CONNECTIONS` set to the load-tested number | [performance-tuning.md](./performance-tuning.md) |

Sign-off: engineering ▢  security ▢  operations ▢ — date: ______

When all twenty are checked, flip DNS/LB to production and keep
[silo-panel-ops.md](./silo-panel-ops.md) open for the first hour.
