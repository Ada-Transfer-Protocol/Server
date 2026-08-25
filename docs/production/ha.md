# High Availability — honest scope for v1

**The short version:** AdaTP v1 is a single-node server with in-memory
state. You can build fast *recovery* around it; you cannot build
zero-interruption *failover*, because no mechanism exists to share rooms,
sessions or presence between two processes. This page describes what works
today and names what does not exist.

> **Status:** fast *recovery* — Operator-builds (patterns below). Zero-downtime
> *failover* and multi-node rooms — **Roadmap** (a state backplane;
> [`../../ROADMAP.md`](../../ROADMAP.md)). This page promises neither.

## Not available in v1 (do not design around these)

- Clustering / multi-node rooms — two servers are two separate worlds.
- Room/session state replication or handoff.
- Message persistence or replay (delivery is at-most-once —
  [`../architecture/reliability.md`](../architecture/reliability.md)).
- Sticky-session horizontal scaling (there is nothing to stick between).

These are roadmap items, not configuration you missed.

## What a failure actually costs

When the process dies or is replaced:

- Every WebSocket drops. Clients reconnect (SDK apps should implement
  reconnect-with-backoff), re-authenticate, re-join their rooms.
- Room membership, presence and in-flight packets are gone. Application
  state must be reconstructable by clients (e.g. GameState full-snapshot
  convention re-seeds the board on rejoin).
- API keys and webhook endpoints survive (SQLite), users file survives.

Framing:
- **RPO — not applicable for messages** (nothing persisted, nothing to lose
  but in-flight traffic); RPO for config = age of your last SQLite backup.
- **RTO = restart + reconnect time** — typically seconds: process start is
  sub-second, then client reconnect storm.

## Pattern 1 — supervised restart (the baseline everyone needs)

systemd `Restart=always` ([install-binary.md](./install-binary.md)) or
container `restart: unless-stopped` / K8s liveness probe. Combined with
client-side reconnect this yields "seconds of blip" recovery from crashes.

## Pattern 2 — active–passive behind a load balancer

```
              LB (health: GET /readyz)
              ┌──────────┴──────────┐
        ┌─────▼─────┐         ┌─────▼─────┐
        │ adatp A   │         │ adatp B   │
        │ (active)  │         │ (standby) │
        └───────────┘         └───────────┘
        state: A's memory      state: empty
```

- Both nodes run the same config; **each has its own SQLite** — sync
  config-plane changes by re-applying them to both (create webhooks twice)
  or by shipping the backup ([backup.md](./backup.md)). There is no
  built-in replication.
- LB health = `/readyz`, one backend active at a time (priority/failover
  pool, not round-robin — round-robin would split users across two worlds).
- **Planned failover** (patch night):
  1. Drain A: `POST /admin/v1/drain {"enabled":true}` → A's `/readyz` = 503
     → LB shifts new connections to B.
  2. Existing sessions on A finish or are cut with
     `{"enabled":true,"disconnect_clients":true}`.
  3. Work on A; release drain to fail back (or keep B active).
- **Unplanned failover**: A dies → LB health fails → new connections land
  on B. Users re-join into empty rooms on B. In-flight state is lost —
  that is the accepted cost, say it in your SLA.

## Pattern 3 — blue-green deploys

Same mechanics as planned failover with B running the new version:
drain A → probe B with `adatp-cli` end-to-end → shift → keep A as instant
rollback for one release cycle. Full procedure:
[upgrade-rollback.md](./upgrade-rollback.md).

## Scaling beyond one node (sharding, app-level)

If one node's capacity is exceeded ([sizing.md](./sizing.md)), split by
**tenant/room-space at the edge**: `eu.realtime…` / `us.realtime…`, or
route `/ws` by tenant header to different backends. Rooms never span
shards; place each tenant wholly on one node. This is DNS/LB design, not an
AdaTP feature — but it is the honest way to scale v1.

## Client-side requirements for any HA story

Reconnect loop with jittered backoff; re-auth; re-join rooms;
re-announce presence (`DISCOVERY:WHO_IS_HERE`); GameState producers resend
a full snapshot after rejoin. Put these in your client acceptance tests —
server-side HA is worthless if clients don't reconnect cleanly.
