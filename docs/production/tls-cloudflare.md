# TLS Termination — Cloudflare, nginx, caddy

## Why TLS is REQUIRED (not "recommended")

AdaTP's session encryption uses an **unauthenticated** X25519 key agreement:
it encrypts the transport but cannot by itself detect an active
man-in-the-middle ([`../spec/08-security.md`](../spec/08-security.md)).
TLS (`wss://`) provides the endpoint authentication AdaTP defers to the
edge. Production without TLS is misconfiguration, full stop.

Pattern: clients connect `wss://realtime.example.com/ws` (443) → edge
terminates TLS → forwards plain `ws://origin:3000/ws`.

## Option A — Cloudflare (proxied)

1. DNS record for `realtime.example.com`, **proxied** (orange cloud).
2. WebSockets are supported on all plans (Network → WebSockets: On for
   legacy dashboards).
3. SSL/TLS mode: **Full (strict)** — install an Origin CA certificate on
   your own reverse proxy in front of 3000, or point Cloudflare at a
   proxy that already has a valid cert. Never use "Flexible" (plaintext
   Cloudflare→origin crosses the internet).
4. Firewall the origin so only Cloudflare IP ranges reach the proxy port.

Notes: Cloudflare imposes idle timeouts on WS (~100 s proxied); the server's
30 s protocol pings keep healthy connections under that. `/admin/v1` and
`/silo` SHOULD NOT be exposed through the public hostname — see the
location blocks below and [silo-panel-ops.md](./silo-panel-ops.md).

## Option B — nginx

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    http2 on;
    server_name realtime.example.com;

    ssl_certificate     /etc/letsencrypt/live/realtime.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/realtime.example.com/privkey.pem;

    # Data plane
    location /ws {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 120s;      # > 30s server ping interval
        proxy_send_timeout 120s;
    }

    # Health for the LB
    location = /healthz { proxy_pass http://127.0.0.1:3000; }
    location = /readyz  { proxy_pass http://127.0.0.1:3000; }

    # Control plane: deny publicly; reach it via VPN/internal vhost instead.
    location /admin/ { deny all; }
    location /silo   { deny all; }
    location /api/   { deny all; }
}
```

Internal operator vhost (VPN interface only) can proxy `/admin/` and `/silo`
the same way — both are plain HTTP + SSE (SSE needs
`proxy_buffering off;` on the `/admin/v1/logs/stream` path to stream
promptly).

## Option C — caddy

```caddy
realtime.example.com {
    @ops path /admin/* /silo* /api/*
    respond @ops 403

    reverse_proxy 127.0.0.1:3000     # WS upgrade is automatic
}

ops.internal.example.com {           # bind this site to the VPN interface
    reverse_proxy 127.0.0.1:3000 {
        flush_interval -1            # stream SSE immediately
    }
}
```

## Client URLs after TLS

| Client | URL |
| :-- | :-- |
| Browser SDK | `new AdaTPChat("wss://realtime.example.com/ws", …)` |
| Node | `new AdaTPClient("wss://realtime.example.com/ws")` |
| Python | `AdaTPClient(url="wss://realtime.example.com/ws")` |
| PHP | `new Client("wss://realtime.example.com/ws")` |
| C / Arduino | `ws://` only — terminate TLS on a trusted network segment, or front them with a local proxy |

## Verification

```bash
# handshake through the edge:
cargo run -p adatp-cli -- -a wss://realtime.example.com/ws -u user1 -p password123
# confirm the origin port is NOT reachable publicly:
curl -m 3 http://realtime.example.com:3000/healthz && echo "FIREWALL GAP!"
```
