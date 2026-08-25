# Sizing & Capacity

There is no magic connection number. AdaTP's cost is dominated by **fan-out**,
not connection count — size from your traffic shape, then verify with the
bundled load tester.

## What actually drives load

| Driver | Cost model |
| :-- | :-- |
| Room broadcast | one inbound message becomes `members` outbound messages (sender included). Outbound msg/s ≈ Σ over rooms of `members × per-member send rate × members` |
| Voice | continuous: each speaker emits ~8 packets/s of 4 096 B PCM (2048 samples @16 kHz) → ~33 kB/s upstream per speaker, × members downstream |
| File transfer | bursts of 16 KiB chunks, broadcast to the whole room — put transfers in small/dedicated rooms |
| Encryption | AES-GCM per packet **per recipient** for secure sessions (plaintext sessions are forwarded without re-encryption) |
| Plugins | veto hooks add up to `hook_timeout_ms` of latency budget per message *only* for hooked types (text/file/auth/tools) |
| Webhooks | off the hot path entirely (async queue) |

Rule of thumb: 10 rooms × 10 members × 1 msg/s each = 1 000 deliveries/s —
trivial. One room with 500 members where everyone types = 250 000
deliveries/s — that is the shape that hurts, regardless of hardware.

## Measured reference point

From `tools/loadtest` on a developer laptop, **debug build** (release builds
are substantially faster):

```
40 clients × 10 msg/s across 4 rooms (≈10 members each), 64 B payloads
→ 394 msg/s in, ~3 941 deliveries/s out
→ latency p50 3 ms · p95 6 ms · p99 8 ms · zero errors, zero drops
```

Treat this as a floor, not a promise. Reproduce on *your* hardware:

```bash
cd tools/loadtest && npm install
node loadtest.mjs --url ws://HOST:3000/ws \
  --clients 200 --rooms 20 --rate 5 --duration 60 \
  --username user1 --password password123
```

Watch during the run: Silo → OVERVIEW (throughput chart, DROPPED MSGS) or
`GET /admin/v1/load`.

## Memory

Per-connection state is small (session keys, a 256-message outbound queue,
registry entry), but the honest method is to measure, not to trust a table:

```bash
# steady-state baseline
ps -o rss= -p $(pgrep adatp-server)
# run loadtest with N connections, sample again, divide the delta by N
```

Budget headroom for bursts: outbound queues are allocated lazily but can
momentarily hold 256 messages × payload size per slow consumer.

## Signals that you are at capacity

| Signal | Where | Meaning |
| :-- | :-- | :-- |
| `dropped_messages` climbing | `/api/metrics`, Silo KPI | consumers can't keep up with fan-out — shrink rooms or lower rates |
| p99 latency rising in loadtest | loadtest output | CPU saturation on the broadcast path |
| `capacity_used_pct` | `/admin/v1/lb-hints` | connections vs `MAX_CONNECTIONS` — **a reporting hint only; the server does not enforce this cap** |
| file descriptors near `ulimit -n` | OS | raise the limit ([performance-tuning.md](./performance-tuning.md)) |

## Sizing procedure

1. Model your worst room (members × rate) and total concurrent connections.
2. Deploy a **release build** on production-shaped hardware.
3. Run `tools/loadtest` at 2× the modeled peak for ≥ 5 minutes.
4. Accept only if: zero connect failures, `dropped_messages` flat, p99 within
   your budget.
5. Set `MAX_CONNECTIONS` to the accepted connection count so `lb-hints`
   reports honest capacity to your load balancer automation.

Scaling beyond one node means sharding by room/tenant across independent
servers at your edge — v1 has no clustering ([ha.md](./ha.md)).
