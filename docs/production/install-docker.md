# Install — Docker

## Image

`server/Dockerfile` is a two-stage build: `rust:1.83-slim-bookworm` compiles
offline against the vendored crates; the runtime stage is
`debian:bookworm-slim` with only `ca-certificates` + `libssl3`, running as
the non-root user `adatp`.

```bash
cd server
docker build -t adatp-server:1.0.0 .
```

Image facts:

| Property | Value |
| :-- | :-- |
| Listen | `3000` (`EXPOSE 3000`) |
| User | `adatp` (non-root) |
| State volume | `/app/data` (`DATABASE_URL=sqlite:/app/data/adatp.db`) |
| Users file | `/app/users.json` (the **demo** file is baked in — override it) |
| Healthcheck | `adatp-server --healthcheck` (self-probe of `/healthz`; no curl in the image) |

## Compose

`deploy/docker/docker-compose.yml` is the canonical deployment:

```bash
cd deploy/docker
docker compose up --build -d
docker compose ps          # → healthy within ~15s
curl -s http://127.0.0.1:3000/healthz
```

Production adjustments to make in the compose file:

```yaml
services:
  adatp:
    image: adatp-server:1.0.0          # pin the tag, never :latest
    stop_grace_period: 10s
    environment:
      AUTH_DRIVER: api
      AUTH_API_URL: https://auth.internal.example.com/verify
      ADMIN_TOKEN: ${ADATP_ADMIN_TOKEN}   # from your secret store / .env
      RUST_LOG: warn
    volumes:
      - adatp-data:/app/data
      - ./users.json:/app/users.json:ro   # only if you use the file driver
      - ./plugins:/app/plugins:ro         # optional; set PLUGINS_DIR=/app/plugins
    ports:
      - "127.0.0.1:3000:3000"             # publish only to the proxy host
```

> Graceful shutdown (plugin notification, client `Disconnect` frames)
> runs on both **SIGTERM and SIGINT** since v1.0.0, so a plain
> `docker stop` drains cleanly within the default 10 s grace period.

> The bundled example plugins need `node`, which the slim runtime image does
> **not** include. Either build a derived image that adds Node.js, ship
> plugins written in a language present in your image, or run without
> plugins (the platform stays idle if the directory is absent).

## Registry

```bash
docker tag adatp-server:1.0.0 registry.example.com/adatp/adatp-server:1.0.0
docker push registry.example.com/adatp/adatp-server:1.0.0
```

Pin deployments to the immutable tag (or digest). Keep the previous tag
available for rollback ([upgrade-rollback.md](./upgrade-rollback.md)).

## API keys inside the container

The image ships only the server binary. Manage `/api/*` keys from a host
checkout against the mounted DB, with the container stopped or the DB
briefly copied:

```bash
docker compose stop adatp
sqlite3 "$(docker volume inspect docker_adatp-data -f '{{.Mountpoint}}')/adatp.db" \
  "UPDATE api_keys SET is_active=0 WHERE key='admin-secret-key';"
docker compose start adatp
```

(or run `adatp-admin` from a source checkout with
`--db-url sqlite:/path/to/volume/adatp.db`).

## Logs

Everything goes to stderr → the Docker log driver:

```bash
docker compose logs -f adatp
```

Ship them with your usual driver (`json-file` + rotation, `journald`,
`fluentd`, …). The same stream is visible live in Silo → LOGS
([observability.md](./observability.md)).

## Verify

```bash
curl -s http://127.0.0.1:3000/readyz
# from a source checkout:
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

Then complete [security-hardening.md](./security-hardening.md) — at minimum
rotate the bootstrap `admin-secret-key` and set a strong `ADMIN_TOKEN`.
