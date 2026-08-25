# Upgrade & Rollback

Every upgrade is a restart: sessions drop, rooms restart empty
([ha.md](./ha.md)). The procedure below turns that into a controlled,
announced blip instead of a surprise.

## Preparation

- [ ] New artifact built and verified: `cargo build --release --offline` +
      `cargo test --workspace --offline`, or the pinned image tag pulled.
- [ ] Read the release CHANGELOG for breaking notes (v1.0.0 itself
      renumbered message types and renamed the admin binary — SDKs and
      server must move together across such releases).
- [ ] SQLite backup taken now ([backup.md](./backup.md)). Schema changes
      have been additive-only; older-DB-into-newer-server is the supported
      direction.
- [ ] Previous artifact retained and startable (that *is* the rollback).
- [ ] Maintenance window announced if your product needs it.

## Procedure

```bash
export A="http://127.0.0.1:3000"; export T="x-admin-token: $ADMIN_TOKEN"

# 1. Engage drain — readiness flips 503, LB stops sending new connections,
#    new WS attempts are refused, existing sessions continue.
curl -s -H "$T" -X POST $A/admin/v1/drain \
     -H 'content-type: application/json' -d '{"enabled":true}'

# 2. Watch connections fall (users finishing naturally):
watch -n 5 'curl -s -H "x-admin-token: $ADMIN_TOKEN" '"$A"'/admin/v1/overview | jq .connections.active'

# 3. Past your patience threshold, cut the stragglers gracefully
#    (server sends Disconnect frames):
curl -s -H "$T" -X POST $A/admin/v1/drain \
     -H 'content-type: application/json' -d '{"enabled":true,"disconnect_clients":true}'

# 4. Stop / replace / start
sudo systemctl stop adatp-server
sudo cp target/release/adatp-server /opt/adatp/bin/adatp-server   # binary path
# docker: docker compose pull && docker compose up -d              # image path
sudo systemctl start adatp-server

# 5. Verify before taking traffic (drain state cleared by restart,
#    but the LB may need a probe cycle):
curl -s $A/healthz && curl -s $A/readyz
/opt/adatp/bin/adatp-cli -a 127.0.0.1:3000 -u user1 -p '<pw>'   # end-to-end probe
curl -s -H "$T" $A/admin/v1/plugins | jq '.plugins[] | {name,state}'   # plugins running
curl -s -H "$T" -X POST $A/admin/v1/webhooks/<one-id>/test              # a signed delivery

# 6. Drain is already released by the restart; confirm:
curl -s $A/readyz          # {"status":"ready"}
```

Note: drain state is **in-memory** — the restart in step 4 clears it. If
you must boot the new version isolated (e.g. to run checks before the LB
sees it), re-engage drain immediately after start and release it at the
end.

## Rollback

Same procedure with the previous artifact in step 4:

```bash
sudo systemctl stop adatp-server
sudo cp /opt/adatp/bin/adatp-server.prev /opt/adatp/bin/adatp-server
# docker: docker compose up -d  (compose file pinned back to the prior tag)
sudo systemctl start adatp-server
```

- Keep exactly one known-good previous artifact per environment
  (`adatp-server.prev`, or the previous image tag — never deploy `:latest`).
- DB: no action for additive schemas. If a future release documents a
  migration, restore the pre-upgrade backup when rolling back across it.
- SDK compatibility: rolling the *server* back across a protocol-breaking
  release while clients already shipped the new SDK will break them —
  coordinate rollback windows with client releases.

## Blue-green variant

With two nodes ([ha.md](./ha.md) pattern 3): deploy new version to the
standby, run step 5's verification against it directly (port-forward or
internal vhost), then drain the active node and let the LB shift. Rollback
= shift back; the old node is untouched.

## Post-upgrade watch (first 30 minutes)

- Silo OVERVIEW: connections recovering to the normal band, DROPPED MSGS
  flat, plugins m/n complete, no red breaker LEDs.
- `journalctl -u adatp-server --since -30min | grep -Ei 'error|panic'` — empty.
- One real client path exercised (your app's smoke test, or the
  `adatp-cli` probe from monitoring).
