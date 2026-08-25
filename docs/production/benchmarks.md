# Benchmarks

> **No performance numbers are published here yet.** The tables below are
> intentionally empty. Run the harness on your own build and hardware and fill
> them with REAL measured values — do not copy numbers from anywhere else, and
> do not treat any figure quoted elsewhere in the docs as an official
> benchmark. Latency and throughput depend entirely on hardware, build profile
> (debug vs `--release`), room fan-out, and payload size.

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

## Results (fill in — currently unpublished)

**Environment** (fill in every field before sharing results):

| Field | Value |
| :-- | :-- |
| CPU (model / cores) | _tbd_ |
| RAM | _tbd_ |
| OS / kernel | _tbd_ |
| Server build | _tbd_ (`--release` recommended) |
| Deployment | _tbd_ (native / docker / k8s) |
| AdaTP version | _tbd_ |
| Date | _tbd_ |

**Measured** (one row per concurrency level; copy from the generated `.md`):

| clients | connected | sent/s | recv/s | p50 ms | p95 ms | p99 ms | CPU avg % | CPU peak % | RSS avg MB | RSS peak MB |
| --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| _tbd_ | | | | | | | | | | |
| _tbd_ | | | | | | | | | | |
| _tbd_ | | | | | | | | | | |

## Reading the numbers honestly

- **Deliveries ≫ sends.** Each message fans out to every other member of its
  room, so `recv/s` scales with room size. Report `--rooms` and `--rate` next
  to any throughput figure or it is meaningless.
- **Watch `dropped_messages`.** Check `/api/metrics` after a run
  ([observability.md](./observability.md)); non-zero drops mean the server shed
  load for slow consumers and the latency percentiles understate real backlog.
- **Single node.** AdaTP v1 is one process; these numbers do not extrapolate
  across replicas (there is no clustering — see
  [ha.md](./ha.md)).
- **Debug builds are far slower** than `--release`. Always state the profile.
