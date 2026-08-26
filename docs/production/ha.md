# High Availability — honest scope for v1

**The short version:** AdaTP now supports **multi-node rooms** via a Redis
routing backplane (`ADATP_BACKPLANE_URL`) — a room broadcast fans out to clients
on every node, so you can scale room messaging horizontally behind a load
balancer. What is **still** per-node: session/presence state and membership
counts (a node only tracks its own connections), so there is no zero-interruption
*failover handoff* of live sessions. This page describes what works today and
names what does not exist.

> **Status:** **multi-node room messaging — available** (Redis backplane, tested:
> [`../../tests/backplane/`](../../tests/backplane/)). Fast *recovery* —
> operator-builds (patterns below). Zero-downtime *failover* of live sessions and
> cross-node presence/membership — **still roadmap**. This page promises only
> the first.

## Available now — multi-node room messaging (the backplane)

Set `ADATP_BACKPLANE_URL=redis://host:port` on every node and point them at one
Redis. Each node publishes its room broadcasts to a shared channel and
re-delivers what it receives to its own local connections. Result: a client on
node A and a client on node B in the same room exchange messages. Put the nodes
behind any load balancer (no sticky sessions required for messaging). Delivery is
best-effort (same as the in-process queues); secure the Redis link at the network
layer (the routed payload is plaintext, as it is in-process — AdaTP is
hop-by-hop, not E2E).

## Still not available (do not design around these)

- **Cross-node presence / membership views** — each node reports only its own
  connections; `list_connections`/room counts are per-node.
- **Session state replication or live-session failover handoff** — if a node
  dies, its clients reconnect (to any node) and re-join; in-flight state is gone.
- Message persistence or replay (delivery is at-most-once —
  [`../architecture/reliability.md`](../architecture/reliability.md)).

These remain roadmap items, not configuration you missed.

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

## Scaling beyond one node

Two options, depending on whether rooms must span nodes:

1. **Backplane (rooms span nodes).** Run N nodes with `ADATP_BACKPLANE_URL`
   pointed at one Redis, behind a plain load balancer. Room messages fan out
   across all nodes — no sharding, no sticky sessions. This is the direct answer
   to "one node isn't enough."
2. **Edge sharding (rooms stay on one node).** If you prefer isolation over a
   shared Redis, split by **tenant/room-space at the edge**: `eu.realtime…` /
   `us.realtime…`, or route `/ws` by tenant header to different backends; place
   each tenant wholly on one node. This is DNS/LB design, not an AdaTP feature.

Capacity per node: [sizing.md](./sizing.md).

## Client-side requirements for any HA story

Reconnect loop with jittered backoff; re-auth; re-join rooms;
re-announce presence (`DISCOVERY:WHO_IS_HERE`); GameState producers resend
a full snapshot after rejoin. Put these in your client acceptance tests —
server-side HA is worthless if clients don't reconnect cleanly.
