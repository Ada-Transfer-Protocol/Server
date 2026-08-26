# Benchmarks

> **The numbers below are REAL, measured by the harness** on the environment
> named in the table — treat them as a **reference sample, not an official spec**.
> They were taken on a **debug build** on a laptop (a `--release` build is
> faster), single node. Reproduce on your own build and hardware; do not treat
> any figure quoted elsewhere in the docs as an official benchmark. Latency and
> throughput depend entirely on hardware, build profile (debug vs `--release`),
> room fan-out, and payload size.

## What is measured

The harness drives the server with real WebSocket clients that authenticate,
join rooms, and exchange text messages. Each message carries a send timestamp,
so every delivery yields an end-to-end latency sample
(send → server → broadcast → receive). It reports:

- **Latency** p50 / p95 / p99 (ms), end to end.
- **Throughput** messages sent/s and delivered/s (deliveries fan out per room).
- **Connections** established vs. requested, and socket errors.
- **Server CPU %** (avg + peak) and **RSS MB** (avg + peak), sampled once per
  second — only when the server process is observable (see below).

## Tooling

| File | Role |
| :-- | :-- |
| `tools/loadtest/loadtest.mjs` | the load generator (Node + `ws`); prints p50/p95/p99 and throughput |
| `tools/loadtest/run-benchmark.sh` | reproducible wrapper: sweeps concurrency, samples CPU/RAM, emits CSV + Markdown |

## Reproduce

1. Start a server to test. For a realistic run use the release build or the
   container, not a debug build:

   ```bash
   # native release build
   cargo run -p adatp-server --release
   # …or the container (see install-docker.md)
   docker compose up --build -d
   ```

2. Make sure a test account exists (the demo `users.json` ships `user1` /
   `password123`). For the container, the demo file is baked in.

3. Run the sweep. Pass `--server-pid` (native) or `--docker <container>` so
   CPU/RAM are recorded; omit both and those columns are reported as `NA`:

   ```bash
   cd tools/loadtest

   # native server, sweep 50→200 clients, 30s each:
   ./run-benchmark.sh --clients 50,100,200 --duration 30 \
     --url ws://127.0.0.1:3000/ws --server-pid "$(pgrep -f adatp-server | head -1)"

   # containerised server:
   ./run-benchmark.sh --clients 50,100,200 --duration 30 \
     --docker "$(docker compose ps -q adatp-server)"
   ```

   Each run writes `tools/loadtest/results/bench-<timestamp>.csv` and a matching
   `.md` table. Paste that table below.

> Note: `loadtest.mjs` speaks **plaintext** `ws://` for a clean measurement of
> the server itself. To include TLS-termination overhead, point `--url` at the
> proxy (`wss://localhost/ws`) from [install-docker.md](./install-docker.md);
> Node must trust the proxy's certificate (or set
> `NODE_TLS_REJECT_UNAUTHORIZED=0` for a local self-signed run only).

## Results — reference sample (reproduce on your own hardware)

**Environment:**

| Field | Value |
| :-- | :-- |
| CPU (model / cores) | Apple M4 Pro / 14 cores |
| RAM | 48 GB |
| OS / kernel | macOS (Darwin 25.3.0) |
| Server build | **debug** (`cargo build`) — `--release` would be faster |
| Deployment | native, single node, `MSG_RATE_LIMIT=0`, `AUTH_DRIVER=none` |
| AdaTP version | 1.2.0 |
| Date | 2026-08-26 |

**Measured** — each run: `--rooms 10 --rate 10 --duration 10`, so every message
fans out to ~(clients/10) room members (that is why `recv/s ≫ sent/s`):

| clients | connected | sent/s | recv/s | p50 ms | p95 ms | p99 ms | CPU avg % | CPU peak % | RSS avg MB | RSS peak MB |
| --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| 100 | 100/100 | 979 | 9772 | 7 | 16 | 19 | 37.3 | 46.0 | 21.2 | 21.8 |
| 250 | 250/250 | 2440 | 60994 | 13 | 22 | 27 | 115.0 | 140.4 | 26.2 | 26.8 |
| 500 | 500/500 | 4873 | 243074 | 8 | 15 | 21 | 238.3 | 271.7 | 32.1 | 32.7 |

Read at face value: on this laptop a single **debug** node held 500 concurrent
clients delivering ~243k messages/second (fan-out) at p99 ≈ 21 ms, 0 connection
or socket errors, in ~32 MB RSS. These are honest sample numbers, not a
production-scale (10k/50k) benchmark — that needs dedicated hardware and a
`--release` build. The point of publishing them is that the harness runs and the
tables are no longer empty; run it on your target to get numbers that bind.

## Reading the numbers honestly

- **Deliveries ≫ sends.** Each message fans out to every other member of its
  room, so `recv/s` scales with room size. Report `--rooms` and `--rate` next
  to any throughput figure or it is meaningless.
- **Watch `dropped_messages`.** Check `/api/metrics` after a run
  ([observability.md](./observability.md)); non-zero drops mean the server shed
  load for slow consumers and the latency percentiles understate real backlog.
- **Single node here.** These numbers are one process. Multiple nodes can now
  share rooms via the Redis backplane (`ADATP_BACKPLANE_URL` — see
  [ha.md](./ha.md)), which adds a Redis hop to cross-node deliveries; benchmark
  that topology separately rather than extrapolating these single-node figures.
- **Debug builds are far slower** than `--release`. Always state the profile.
