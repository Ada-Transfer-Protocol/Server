# Security Hardening Checklist

Work through every row before real traffic. Policy background: the
**[security model](../SECURITY_MODEL.md)** (threat model + honest gaps),
the normative [`../spec/08-security.md`](../spec/08-security.md), and the
repository `SECURITY.md`.

> **The one non-negotiable:** terminate **TLS (`wss://`)** in front of the
> server. AdaTP's own crypto is hop-by-hop (not E2E) and its handshake is
> **unauthenticated**, so TLS is what actually authenticates the server and
> stops a man-in-the-middle today. Everything else on this page assumes TLS is
> already in place.

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
- [ ] `MSG_RATE_LIMIT` sized for your data plane (default 200 msg/s per
      connection; `0` disables). Exceeding it closes the connection
      (`rate_limited`) — set it above your busiest legitimate client.
- [ ] `MAX_CONNECTIONS` sized to the host (default 10000, **enforced**: the
      WebSocket upgrade returns HTTP 503 over the cap; slot released on close).
      Pair with `LimitNOFILE`.
- [ ] Room policy set if you need it: `ROOM_ALLOWLIST` (CSV; non-empty
      restricts joinable rooms — **include the default `global` lobby**) and
      `ROOM_PROTECTED_PREFIX` + `ROOM_PROTECTED_ROLE` (prefix-matched rooms
      require the role). Initial auto-placement into `global` is not gated.
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

Stated plainly so nobody discovers them in an audit (full detail +
remediation path in [`../SECURITY_MODEL.md`](../SECURITY_MODEL.md)):

- **No server authentication in the AdaTP layer** — the X25519 handshake is
  unauthenticated. TLS is the MITM defense. (Ed25519 primitives exist in
  `adatp-core` but are not yet wired into the handshake — roadmap.)
- **No end-to-end encryption** — payloads are plaintext in the server process
  (hop-by-hop; re-encrypted per recipient).
- **Header integrity gap (empty AAD)** — the AEAD's AAD is empty, so header
  fields other than the nonce-bound sequence are not authenticated. Anti-replay
  itself **is** enforced (`SecureSession::decrypt` rejects any sequence at or
  below the highest already-accepted; the connection is then closed) — it is the
  header bytes, not replay, that remain uncovered. Roadmap: bind the header as
  AAD.
- **Room-join authorization is default-permissive** — a plugin `join` veto hook
  plus a built-in config policy (`ROOM_ALLOWLIST`, and `ROOM_PROTECTED_PREFIX` /
  `ROOM_PROTECTED_ROLE`) can restrict joins, but with none configured any
  authenticated user may join any room name. The initial auto-placement into the
  default `global` room is **not** policy-gated, so include `global` in any
  `ROOM_ALLOWLIST`. You can still gate room access in your `api` backend/gateway
  or use unguessable names.
- **No L3/L4 volumetric flood scrubbing** — a per-connection message-rate limit
  (`MSG_RATE_LIMIT`, default 200 msg/s; `0` disables) and an enforced
  total-connection cap (`MAX_CONNECTIONS`, default 10000) now bound an
  established connection's data plane and the total socket count, but the server
  still does not absorb network-layer packet/SYN floods or connection churn.
  Absorb those at the edge/CDN.
- **File auth driver stores plaintext passwords** — development/demo only; use
  `AUTH_DRIVER=api` in production.
- **One shared admin token** — no per-operator identity; front `/admin` with an
  authenticating proxy if you need attribution.
