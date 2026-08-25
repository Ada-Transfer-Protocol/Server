# Security Hardening Checklist

Work through every row before real traffic. Policy background:
[`../spec/08-security.md`](../spec/08-security.md) and the repository
`SECURITY.md`.

## 1. Transport

- [ ] TLS terminates at the edge; clients use `wss://` only
      ([tls-cloudflare.md](./tls-cloudflare.md)). AdaTP's own encryption is
      defense-in-depth, **not** a TLS substitute (unauthenticated DH).
- [ ] Origin port 3000 unreachable from the internet: bind `HOST=127.0.0.1`
      behind a local proxy, or firewall 3000 to the LB/Cloudflare ranges.
      Verify: `curl -m3 http://<public-ip>:3000/healthz` fails from outside.
- [ ] `/admin/v1`, `/silo`, `/api` blocked on the public vhost; reachable
      only via VPN/internal ingress with your SSO in front.

## 2. Credentials & tokens

- [ ] **Rotate the bootstrap API key.** First boot creates
      `admin-secret-key` (logged). Replace and revoke it:
      ```bash
      adatp-admin auth create --description "metrics-scraper" --db-url sqlite:/var/lib/adatp/adatp.db
      adatp-admin auth list   --db-url sqlite:/var/lib/adatp/adatp.db   # find the default's id
      adatp-admin auth revoke <id-of-default> --db-url sqlite:/var/lib/adatp/adatp.db
      ```
- [ ] `ADMIN_TOKEN` set explicitly, ≥32 random chars, sourced from a secret
      manager, never in shell history or compose files committed to git.
      (Unset = regenerated every boot and printed to logs — dev behavior only.)
- [ ] `AUTH_DRIVER=api` in production; the demo `users.json`
      (well-known passwords) deleted from production hosts; the file driver
      reserved for isolated dev/staging.
- [ ] One credential per bot/integration, least role
      ([auth-providers.md](./auth-providers.md)).
- [ ] Webhook signing secrets in the secret store; consumers verify
      signatures ([webhooks-ops.md](./webhooks-ops.md)).

## 3. Server configuration

- [ ] `ADATP_WEBHOOK_ALLOW_PRIVATE` **unset** (a boot-log warning appears
      when it is active — alert on that line).
- [ ] `RUST_LOG=warn` (info leaks usernames/rooms into logs at volume;
      keep info only where your log pipeline is access-controlled).
- [ ] `MAX_FRAME_BYTES` left at 1 MiB unless you need more — it is your
      memory-amplification bound per message.
- [ ] `.env` file `chmod 600`, owned by root, `EnvironmentFile=` in the
      unit (not `Environment=` lines visible in `systemctl show`).

## 4. Plugins

- [ ] Every deployed plugin's manifest reviewed: permissions minimal,
      `hook_failure_policy` chosen deliberately, rate limits set.
- [ ] Third-party plugin code vendored + pinned; no runtime downloads.
- [ ] OS-level resource limits around server+plugins (systemd slice /
      container limits) — [plugins-ops.md](./plugins-ops.md).
- [ ] Egress restrictions for untrusted plugins (the SSRF guard covers
      webhooks only, not plugin processes).

## 5. Host / OS

- [ ] Dedicated non-root user (`adatp`), `NoNewPrivileges=true`,
      `ProtectSystem=full` in the unit ([install-binary.md](./install-binary.md));
      containers already run non-root.
- [ ] `LimitNOFILE=65536` (or sized to your connection target).
- [ ] Automatic security updates for the base OS / rebuilds for the base
      image (`debian:bookworm-slim`, `libssl3`).
- [ ] Backups encrypted at rest — they contain webhook secrets and (file
      driver) plaintext passwords ([backup.md](./backup.md)).

## 6. Dependencies

- [ ] The Rust dependency tree is **vendored in-repo** (`server/vendor/`,
      337 crates) — builds are reproducible and offline; audit/update the
      vendor directory deliberately, never ad hoc on a prod host.
- [ ] SDK dependency updates flow through your normal review (npm/pip/
      composer lockfiles).

## 7. Verification & response

- [ ] `bash tests/integration/run.sh` green on the release artifact
      (includes auth-refusal, drain, kick, HMAC verification tests).
- [ ] Credential-leak drill rehearsed: rotate `ADMIN_TOKEN` (restart) →
      rotate API keys (`adatp-admin`) → rotate webhook endpoints →
      revoke affected users at the IdP → review Silo LOGS + webhook audit
      ([incident-runbook.md](./incident-runbook.md)).
- [ ] Vulnerability reports channel published (repository `SECURITY.md`).

## Known limitations to carry into your threat model

Stated plainly so nobody discovers them in an audit: AdaTP session crypto
does not authenticate the server (TLS does); message payloads are visible
to the server process (hop-by-hop, no E2E); replay protection is
best-effort sequence tracking; the file auth driver stores plaintext
passwords; admin actions share one token (no per-operator identity —
front with an authenticating proxy if you need attribution).
