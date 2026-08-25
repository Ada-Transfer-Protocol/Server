# AdaTP Quickstart — native & Docker

Both paths end with the server on **port 3000**: WebSocket data plane at
`/ws`, health at `/healthz`, readiness at `/readyz`.

## Path A — native (cargo)

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo build --release --offline        # all crates are vendored
HOST=0.0.0.0 PORT=3000 ./target/release/adatp-server
```

Configuration comes from environment variables or a `.env` file
(see `server/.env.example`). The default auth driver is `file` with the demo
`server/users.json` — dev only; use `AUTH_DRIVER=api` in production.

Verify:

```bash
curl http://127.0.0.1:3000/healthz          # {"status":"ok"}
curl http://127.0.0.1:3000/readyz           # {"status":"ready"}
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

The last command performs a real X25519 handshake and an encrypted login —
if it prints `Login OK`, the deployment works end to end.

> Port 3000 already taken locally? `PORT=3100 ./target/release/adatp-server`
> and point clients at `ws://127.0.0.1:3100/ws`. See
> [`ports.md`](./ports.md).

## Path B — Docker

```bash
cd deploy/docker
docker compose up --build -d
docker compose ps            # healthcheck turns "healthy" within ~15s
curl http://127.0.0.1:3000/healthz
```

The image runs as a non-root user, persists its SQLite state in the
`adatp-data` volume, and health-checks itself with the built-in
`adatp-server --healthcheck` probe (no curl inside the container).

To use your own users file or an external auth API, edit the environment /
volume blocks in `deploy/docker/docker-compose.yml` (comments included).

## Production edge (TLS)

Terminate TLS on 443 at your load balancer, reverse proxy or Cloudflare and
forward to the container's 3000. Clients then connect with
`wss://your-domain/ws`. The AdaTP session encryption is defense in depth —
TLS in production is REQUIRED (see `docs/spec/08-security.md`).
