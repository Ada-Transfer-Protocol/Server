# Incident Runbook

Exact commands per scenario. Set once per shift:

```bash
export A="http://127.0.0.1:3000"          # or the internal ops vhost
export T="x-admin-token: $ADMIN_TOKEN"
```

---

## 1. Server down / not answering

**Symptoms:** edge health failing, clients cannot connect.

```bash
curl -m 3 $A/healthz                          # process up at all?
systemctl status adatp-server                 # or: docker compose ps / kubectl get pods
journalctl -u adatp-server -n 100 --no-pager  # or: docker compose logs --tail 100 adatp
```

- `AddrInUse` in the log → another process on 3000:
  `lsof -nP -iTCP:3000 -sTCP:LISTEN`; free the port or change `PORT`.
- Panic/OOM → restart is automatic (`Restart=always`); investigate the
  panic line; check memory limits vs [sizing.md](./sizing.md).
- Up but `readyz` 503 `not_ready` → SQLite unreachable → scenario 6.
- Up but `readyz` 503 `draining` → someone left drain engaged → scenario 8.

Recovery is restart + client reconnects (rooms restart empty — expected,
[ha.md](./ha.md)).

## 2. Auth backend down (api driver) — fail-closed

**Symptoms:** every new login gets `AuthFailure {"error":"auth_unavailable"}`
and is disconnected; existing sessions keep working; log shows
`Auth backend unavailable: …`.

```bash
journalctl -u adatp-server --since -15min | grep -c auth_unavailable
curl -m 5 -X POST "$AUTH_API_URL" -H 'content-type: application/json' \
     -d '{"username":"probe","password":"x"}'    # is the IdP answering at all?
```

Mitigation is fixing the backend — that's the point of fail-closed.
If the outage is long and business demands access, the fallback is an
explicit, logged decision: switch to a **freshly provisioned** users file
(`AUTH_DRIVER=file` + temp accounts, never the demo file) and restart;
revert immediately after. Record who decided.

## 3. Webhook target down

**Symptoms:** Silo WEBHOOKS shows red breaker LED; audit full of
`retrying` / `skipped_breaker` / `failed`.

```bash
curl -s -H "$T" $A/admin/v1/webhooks | jq '.webhooks[] | {id,url,breaker_open,failed}'
curl -s -H "$T" $A/admin/v1/webhooks/audit | jq '.audit[-10:]'
# pause the endpoint while the consumer is down:
curl -s -H "$T" -X PATCH $A/admin/v1/webhooks/<id> \
     -H 'content-type: application/json' -d '{"active":false}'
# after recovery:
curl -s -H "$T" -X PATCH $A/admin/v1/webhooks/<id> -H 'content-type: application/json' -d '{"active":true}'
curl -s -H "$T" -X POST  $A/admin/v1/webhooks/<id>/test
```

Events during the outage are **not replayed** ([webhooks-ops.md](./webhooks-ops.md)).

## 4. Plugin crash-loop / misbehaving plugin

```bash
curl -s -H "$T" $A/admin/v1/plugins | jq '.plugins[] | {name,state,restarts,last_error}'
curl -s -H "$T" -X POST $A/admin/v1/plugins/<name>/disable     # stop the bleeding
journalctl -u adatp-server | grep "plugin:<name>" | tail -30    # its stderr
# after a fix is deployed to PLUGINS_DIR/<name>/ :
curl -s -H "$T" -X POST $A/admin/v1/plugins/<name>/reload
```

While disabled: its tools return `tool_failed`; its veto hooks follow
`hook_failure_policy` — if that policy is `deny`, the hooked action is
blocked until re-enabled ([plugins-ops.md](./plugins-ops.md)).

## 5. `dropped_messages` climbing

**Meaning:** some consumer's 256-message outbound queue overflowed — the
server dropped instead of stalling the room.

```bash
curl -s -H "$T" $A/admin/v1/overview | jq '.connections.dropped_messages'
curl -s -H "$T" $A/admin/v1/rooms | jq '.rooms | sort_by(-.members)[:5]'   # biggest fan-out
curl -s -H "$T" $A/admin/v1/connections | jq '.connections | length'
```

- One victim client (bad network) → find it in CONNECTIONS and kick:
  `curl -s -H "$T" -X DELETE $A/admin/v1/connections/<id>`
- Broad climb across a huge room → capacity/shape problem:
  [sizing.md](./sizing.md) (smaller rooms, lower rates) — dropping is the
  designed behavior, the fix is upstream.

## 6. Disk full / SQLite errors

**Symptoms:** `readyz` 503 `not_ready`; webhook CRUD returns `db_error`.
Data plane for already-connected clients keeps running.

```bash
df -h /var/lib/adatp
sqlite3 /var/lib/adatp/adatp.db "PRAGMA integrity_check;"
```

Free space (logs are the usual culprit), then re-check `readyz`. If the DB
is corrupt: restore from backup ([backup.md](./backup.md)) — API keys and
webhooks are the only content.

## 7. Suspected credential leak

Order matters; do all four:

```bash
# 1. Admin token: rotate in the secret store, then
sudo systemctl restart adatp-server           # token is read at boot
# 2. API keys:
adatp-admin auth list   --db-url sqlite:/var/lib/adatp/adatp.db
adatp-admin auth create --description "replacement" --db-url ...
adatp-admin auth revoke <old-id> --db-url ...
# 3. Webhook secrets: create replacement endpoints, verify, delete old
#    (see webhooks-ops.md rotation procedure)
# 4. User credentials: revoke at your IdP (api driver) or edit users.json +
curl -s -H "$T" -X POST $A/admin/v1/users/reload
```

Then review Silo LOGS + webhook audit for what the leaked credential did,
and kick any live session using it (CONNECTIONS → KICK).

## 8. Emergency isolation (stop the world)

```bash
# refuse new connections, keep existing:
curl -s -H "$T" -X POST $A/admin/v1/drain -H 'content-type: application/json' -d '{"enabled":true}'
# …or cut everyone too:
curl -s -H "$T" -X POST $A/admin/v1/drain -H 'content-type: application/json' \
     -d '{"enabled":true,"disconnect_clients":true}'
# release:
curl -s -H "$T" -X POST $A/admin/v1/drain -H 'content-type: application/json' -d '{"enabled":false}'
```

Drain state is in-memory: a restart clears it — re-engage after restarting
if you meant to stay isolated.
