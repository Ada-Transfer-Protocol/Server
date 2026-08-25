# Backup & Restore

AdaTP's runtime state is deliberately ephemeral. Backups cover the small
**configuration plane** — minutes of work, worth doing properly.

## What to back up

| Asset | Where | Contains | Method |
| :-- | :-- | :-- | :-- |
| SQLite database | `DATABASE_URL` path (e.g. `/var/lib/adatp/adatp.db`, Docker volume `/app/data/adatp.db`) | `api_keys`, `webhooks` (endpoint URLs + **signing secrets**) | `sqlite3 .backup` (safe while running) |
| Users file | `AUTH_FILE_PATH` (file driver only) | usernames, plaintext passwords, roles | file copy |
| Plugins | `PLUGINS_DIR` | manifests + code + plugin-private config | file copy / part of your deploy artifact |
| Environment | `/etc/adatp/adatp.env`, compose file, K8s Secret | includes `ADMIN_TOKEN` | your secret manager IS the backup — don't tar secrets around |

## What is NOT backed up — by design

Rooms, membership, presence, messages, files-in-transit, voice: never
persisted, so never backed up. After any restore, clients reconnect into
empty rooms. If your product needs message history, your application layer
records it (e.g. a plugin/webhook consumer writing to your own store).

## Backup procedure (online-safe)

```bash
#!/usr/bin/env bash
set -euo pipefail
TS=$(date -u +%Y%m%dT%H%M%SZ)
DEST=/var/backups/adatp/$TS
mkdir -p "$DEST"

# 1. SQLite — .backup is consistent even while the server writes
sqlite3 /var/lib/adatp/adatp.db ".backup '$DEST/adatp.db'"

# 2. Users file + plugins
cp /var/lib/adatp/users.json "$DEST/" 2>/dev/null || true
tar -C /var/lib/adatp -czf "$DEST/plugins.tgz" plugins

# 3. Integrity manifest
(cd "$DEST" && shasum -a 256 * > SHA256SUMS)
```

Docker: run the same `sqlite3 .backup` against the volume mountpoint, or
`docker compose exec` is not possible (no sqlite3 in image) — copy the file
while briefly stopped, or from the host mountpoint. K8s: `kubectl cp` from
the PVC via a debug pod.

Because the DB contains **webhook signing secrets** and users.json contains
**plaintext passwords**, encrypt backups at rest and restrict access like
any credential store.

Cadence: after every config change (new webhook/API key) + a daily cron.
Retention per your policy; the files are tiny.

## Restore drill (rehearse before you need it)

1. Provision the host/container per [install-binary.md](./install-binary.md)
   / [install-docker.md](./install-docker.md).
2. Stop the server (or start it only afterwards).
3. Put files back:
   ```bash
   sqlite3 /var/lib/adatp/adatp.db ".restore '/var/backups/adatp/<TS>/adatp.db'"
   cp <TS>/users.json /var/lib/adatp/ 2>/dev/null || true
   tar -C /var/lib/adatp -xzf <TS>/plugins.tgz
   ```
4. Restore `ADMIN_TOKEN` + env from the secret manager.
5. Start; verify:
   ```bash
   curl -s http://127.0.0.1:3000/readyz                       # ready (DB probe passes)
   curl -s -H "x-admin-token: $ADMIN_TOKEN" \
        http://127.0.0.1:3000/admin/v1/webhooks               # endpoints present
   curl -s -H "x-api-key: <your-key>" \
        http://127.0.0.1:3000/api/status                      # rotated key still valid
   cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p …  # end-to-end probe
   ```
6. `POST /admin/v1/webhooks/<id>/test` per endpoint — confirms the restored
   secrets still match your consumers.

## Schema note

The SQLite schema is created idempotently at startup (`CREATE TABLE IF NOT
EXISTS`) and has only grown additively so far — restoring an older DB into
a newer server is expected to work; the reverse (newer DB into older
server) is not tested. Version-pin restores to the matching release when
possible ([upgrade-rollback.md](./upgrade-rollback.md)).
