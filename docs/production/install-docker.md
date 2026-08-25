# Install — Docker

## Image

The repo-root [`Dockerfile`](../../Dockerfile) is a two-stage build:
`rust:1.89-slim-bookworm` compiles offline against the vendored crates
(`vendor/` + `.cargo/config.toml`); the runtime stage is `debian:bookworm-slim`
with only `ca-certificates` + `libssl3`, running as the non-root user `adatp`.

```bash
# from the repository root (where the Dockerfile lives)
docker build -t adatp-server:1.0.0 .
```

Image facts:

| Property | Value |
| :-- | :-- |
| Listen | `3000` (`EXPOSE 3000`) — plain HTTP/WebSocket, **no TLS in-process** |
| User | `adatp` (non-root) |
| State volume | `/app/data` (`DATABASE_URL=sqlite:/app/data/adatp.db`) |
| Users file | `/app/users.json` (the **demo** file is baked in — override it) |
| Healthcheck | `adatp-server --healthcheck` (self-probe of `/healthz`; no curl in the image) |

## Compose (with mandatory TLS)

The canonical stack is the repo-root [`docker-compose.yml`](../../docker-compose.yml).
It runs **two** services and demonstrates the required TLS posture:

- `adatp-server` — built from the Dockerfile; **not published to the host**.
- `caddy` — a TLS-terminating reverse proxy on `:443` (and `:80` for the ACME
  challenge / HTTP→HTTPS redirect) that forwards, WebSocket-aware, to
  `adatp-server:3000` over the private compose network.

The server speaks plain HTTP/WS by design; TLS (`wss://`) is **required** in
production because the AdaTP session handshake is unauthenticated on its own
([security-hardening.md](./security-hardening.md)). This compose file makes
that the default: nothing reaches the server except through Caddy.

```bash
# local: Caddy issues a cert from its internal CA for https://localhost
ADATP_ADMIN_TOKEN=$(openssl rand -hex 32) docker compose up --build -d
docker compose ps                       # → adatp-server healthy, caddy up
curl -k https://localhost/healthz       # -k trusts Caddy's local demo CA

# real domain: automatic Let's Encrypt certificate
SITE_ADDRESS=realtime.example.com \
ADATP_ADMIN_TOKEN=$(openssl rand -hex 32) docker compose up --build -d
```

`ADATP_ADMIN_TOKEN` becomes the server's `ADMIN_TOKEN` (admin API auth);
`SITE_ADDRESS` (default `localhost`) selects the Caddy site name and therefore
whether Caddy uses its internal CA or provisions a real ACME certificate.

### Production adjustments

Override settings without editing the tracked file by adding a
`docker-compose.override.yml`:

```yaml
services:
  adatp-server:
    image: adatp-server:1.0.0            # pin the tag, never :latest
    stop_grace_period: 30s
    environment:
      AUTH_DRIVER: api                    # delegate to your IdP endpoint
      AUTH_API_URL: https://auth.internal.example.com/verify
      RUST_LOG: warn
      MAX_CONNECTIONS: "10000"
    volumes:
      - ./users.json:/app/users.json:ro   # only if you keep the file driver
```

> Graceful shutdown (plugin notification, client `Disconnect` frames) runs on
> both **SIGTERM and SIGINT**, so a plain `docker compose stop` drains cleanly
> within the grace period.

> The bundled example plugins need `node`, which the slim runtime image does
> **not** include. Either build a derived image that adds Node.js, ship plugins
> in a language present in your image, or run without plugins (the platform
> stays idle if `PLUGINS_DIR` is absent).

## Registry

```bash
docker tag adatp-server:1.0.0 registry.example.com/adatp/adatp-server:1.0.0
docker push registry.example.com/adatp/adatp-server:1.0.0
```

Pin deployments to the immutable tag (or digest). Keep the previous tag
available for rollback ([upgrade-rollback.md](./upgrade-rollback.md)). The same
image is what [install-kubernetes.md](./install-kubernetes.md) deploys.

## API keys inside the container

The image ships only the server binary. Manage `/api/*` keys from a host
checkout against the mounted DB, with the container stopped or the DB briefly
copied:

```bash
docker compose stop adatp-server
sqlite3 "$(docker volume inspect adatp-server_adatp-data -f '{{.Mountpoint}}')/adatp.db" \
  "UPDATE api_keys SET is_active=0 WHERE key='admin-secret-key';"
docker compose start adatp-server
```

(or run `adatp-admin` from a source checkout with
`--db-url sqlite:/path/to/volume/adatp.db`). The volume name is
`<project>_adatp-data`; confirm with `docker volume ls`.

## Logs

Everything goes to stderr → the Docker log driver:

```bash
docker compose logs -f adatp-server
docker compose logs -f caddy          # TLS / proxy troubleshooting
```

Ship them with your usual driver (`json-file` + rotation, `journald`,
`fluentd`, …). The same stream is visible live in Silo → LOGS
([observability.md](./observability.md)).

## Verify

```bash
curl -k https://localhost/readyz
# from a source checkout, through the TLS proxy:
cargo run -p adatp-cli -- -a wss://localhost/ws -u user1 -p password123
```

Then complete [security-hardening.md](./security-hardening.md) — at minimum
rotate the bootstrap `admin-secret-key` and set a strong `ADMIN_TOKEN`
(`ADATP_ADMIN_TOKEN`). To measure throughput/latency on this image, use
[benchmarks.md](./benchmarks.md).
