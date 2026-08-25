# Performance Tuning

The honest list: AdaTP v1 has few knobs, and the biggest wins are not knobs
at all — they are build type, room shape, and OS limits. Measure before and
after every change with `tools/loadtest` ([sizing.md](./sizing.md)).

## 1. Run a release build (the single biggest lever)

All published reference numbers in these docs are from a **debug** build
(40×10 msg/s → ~3.9 k deliveries/s, p99 8 ms). Release builds
(`cargo build --release`, and the Docker image builds `--release`) are
dramatically faster on the crypto + routing path. Never benchmark or deploy
debug binaries.

## 2. Application shape (second biggest lever)

- **Room size is quadratic-ish**: fan-out ≈ members × per-member rate ×
  members. Ten rooms of 10 beat one room of 100 by ~10×.
- Route file transfers into small/dedicated rooms — every 16 KiB chunk is
  broadcast to all members.
- Plaintext sessions (browser over wss) are cheaper for the server than
  AdaTP-encrypted sessions (per-recipient AES-GCM). Where TLS already
  protects the hop, plaintext AdaTP sessions are a legitimate choice.
- Veto hooks (`text`/`file`/`auth`/`tool_before`) insert a plugin
  round-trip bounded by `hook_timeout_ms` into those paths. Keep hooked
  plugins fast; don't hook `text` with a 500 ms-budget model call — use an
  async pattern (notify + webhook) instead.

## 3. Server env knobs (the complete list)

| Knob | Default | Tuning notes |
| :-- | :-- | :-- |
| `MAX_FRAME_BYTES` | 1 MiB | Lower it (e.g. 64 KiB) on chat-only deployments to cap memory amplification; raise only if you truly ship >1 MiB payloads. |
| `IDLE_TIMEOUT_SECS` | 90 | Lower (e.g. 45) to reap dead mobile connections faster; keep > 2× the 30 s ping interval. |
| `RUST_LOG` | info | `warn` in production — at `info`, every connection/auth/join is a log line, and logging is real work at high connection churn. |
| `MAX_CONNECTIONS` | 10000 | Reporting only (lb-hints); set to your load-tested number so automation sees honest capacity. |

**What does NOT exist** (don't go looking): worker-thread counts (Tokio
defaults to one worker per core — correct), buffer-size envs, queue-size
envs (outbound queue fixed at 256/connection), GC/alloc tuning.

## 4. OS limits (Linux)

```bash
# file descriptors — one per connection + plugins + sqlite
ulimit -n                          # check; systemd: LimitNOFILE=65536
# accept backlog under connection storms (reconnect stampede after restart)
sysctl -w net.core.somaxconn=1024
# TCP buffers for high-throughput voice fan-out (defaults are usually fine)
sysctl net.ipv4.tcp_rmem net.ipv4.tcp_wmem
```

Container equivalents: `--ulimit nofile=65536:65536` /
`LimitNOFILE` in the systemd unit ([install-binary.md](./install-binary.md)).

## 5. Edge

- Keep proxy read/idle timeouts **above 30 s** (the server's ping cadence)
  or the proxy will cycle healthy idle connections
  ([tls-cloudflare.md](./tls-cloudflare.md)).
- Enable TCP keepalive / HTTP/1.1 upgrade passthrough; no proxy buffering
  on the SSE log stream.
- TLS session resumption at the edge cheapens reconnect storms after a
  restart.

## 6. Measuring a change

```bash
# baseline, then after each single change:
node tools/loadtest/loadtest.mjs --url ws://HOST:3000/ws \
  --clients 200 --rooms 20 --rate 5 --duration 120
```

Compare: deliveries/s, p95/p99, `dropped_messages` at `/api/metrics`
before vs after, and `ps -o rss=` for memory. Change one variable at a
time; keep the numbers in your ops journal so capacity claims stay tied to
evidence.
