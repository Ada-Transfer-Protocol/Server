# Install — Native Binary (systemd)

## Prerequisites

- Linux x86_64/aarch64 (Ubuntu/Debian/RHEL/Fedora/Arch/Alpine) or macOS
- Rust toolchain (rustup) — build-time only
- `pkg-config` + OpenSSL headers on Linux (`libssl-dev` / `openssl-devel`)
- `node` on PATH **only if** you deploy the bundled JS example plugins

## Option A — build from source (recommended, auditable)

All crates are vendored in the repo; the build is fully offline:

```bash
git clone https://github.com/Ada-Transfer-Protocol/Server.git
cd Server
cargo build --release --offline
# binaries:
#   target/release/adatp-server   (the server)
#   target/release/adatp-admin    (API-key manager)
#   target/release/adatp-cli      (protocol test tool)
```

### Recommended layout

```bash
sudo useradd --system --home /var/lib/adatp --shell /usr/sbin/nologin adatp
sudo mkdir -p /opt/adatp/bin /etc/adatp /var/lib/adatp/plugins
sudo cp target/release/{adatp-server,adatp-admin,adatp-cli} /opt/adatp/bin/
sudo cp server/users.json /var/lib/adatp/          # dev/demo only — see auth-providers.md
sudo cp -r plugins/* /var/lib/adatp/plugins/       # optional example plugins
sudo chown -R adatp:adatp /var/lib/adatp
```

`/etc/adatp/adatp.env` (chmod 600, root-owned):

```env
HOST=127.0.0.1            # behind a local reverse proxy; 0.0.0.0 if firewalled
PORT=3000
AUTH_DRIVER=api
AUTH_API_URL=https://auth.internal.example.com/verify
DATABASE_URL=sqlite:/var/lib/adatp/adatp.db
PLUGINS_DIR=/var/lib/adatp/plugins
ADMIN_TOKEN=<long-random-secret>
RUST_LOG=warn
```

### systemd unit

`/etc/systemd/system/adatp-server.service`:

```ini
[Unit]
Description=AdaTP realtime server
After=network-online.target
Wants=network-online.target

[Service]
User=adatp
Group=adatp
WorkingDirectory=/var/lib/adatp
EnvironmentFile=/etc/adatp/adatp.env
ExecStart=/opt/adatp/bin/adatp-server
TimeoutStopSec=10
Restart=always
RestartSec=2
LimitNOFILE=65536
NoNewPrivileges=true
ProtectSystem=full

[Install]
WantedBy=multi-user.target
```

> Graceful shutdown (notify plugins, send `Disconnect` to clients) runs
> on both SIGTERM (systemd default) and SIGINT/Ctrl-C.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now adatp-server
```

## Option B — installer script

The repo ships a universal installer that detects the distro, installs build
dependencies, builds, and generates the same systemd service plus the
`adatp-status` / `adatp-log` / `adatp-restart` / `adatp-stop` helper
commands:

```bash
# Read tools/setup.sh before running it — never pipe unreviewed scripts to bash.
curl -sSL https://raw.githubusercontent.com/Ada-Transfer-Protocol/Server/main/tools/setup.sh | bash
```

`tools/install_service.sh` regenerates just the service;
`tools/uninstall.sh` removes everything.

## Verify

```bash
curl -s http://127.0.0.1:3000/healthz          # {"status":"ok"}
curl -s http://127.0.0.1:3000/readyz           # {"status":"ready"}
/opt/adatp/bin/adatp-cli -a 127.0.0.1:3000 -u user1 -p password123
# → "Secure session established" + "Login OK" proves the full path
```

Then immediately rotate the bootstrap API key
([security-hardening.md](./security-hardening.md)):

```bash
cd /var/lib/adatp
/opt/adatp/bin/adatp-admin auth list  --db-url sqlite:/var/lib/adatp/adatp.db
/opt/adatp/bin/adatp-admin auth create --description "ops" --db-url sqlite:/var/lib/adatp/adatp.db
/opt/adatp/bin/adatp-admin auth revoke <id-of-admin-secret-key> --db-url sqlite:/var/lib/adatp/adatp.db
```

## Upgrade in place

Drain first — full procedure in [upgrade-rollback.md](./upgrade-rollback.md):

```bash
curl -X POST http://127.0.0.1:3000/admin/v1/drain \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"enabled":true}'
# build/copy the new binary, then:
sudo systemctl restart adatp-server
```

## macOS note

No installer is provided. Build with cargo as above and run under `launchd`
(a `KeepAlive` LaunchDaemon pointing at the binary with the same env) or a
process manager of your choice. macOS deployments are supported for
development, not recommended as the production platform.
