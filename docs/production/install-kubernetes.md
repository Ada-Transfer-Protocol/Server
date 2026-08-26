# Install — Kubernetes (single replica)

**Read first:** AdaTP v1 keeps rooms and connections in process memory.
Kubernetes cannot change that:

- `replicas: 1`, `strategy: Recreate`. Two replicas would be two unrelated
  chat servers behind one Service — users would land in different "worlds".
- HPA is inappropriate; scaling out requires app-level sharding ([ha.md](./ha.md)).
- Session affinity is irrelevant (there is only one pod).
- A pod restart drops every session (clients reconnect and re-join).

## Manifests

Applyable manifests live in [`deploy/kubernetes/`](../../deploy/kubernetes/) —
plain YAML, no templating engine required:

| File | Kind | Purpose |
| :-- | :-- | :-- |
| `configmap.yaml` | ConfigMap `adatp-config` | non-secret env (only variables the server actually reads) |
| `secret.yaml` | Secret `adatp-secrets` | `ADMIN_TOKEN` (replace the placeholder) |
| `pvc.yaml` | PersistentVolumeClaim `adatp-data` | RWO volume for the SQLite DB |
| `deployment.yaml` | Deployment `adatp-server` | single replica, probes, resources, non-root |
| `service.yaml` | Service `adatp-server` | ClusterIP on port 3000 |

```bash
# 1. Build, tag, and push the image (see install-docker.md), then set the
#    image reference in deploy/kubernetes/deployment.yaml.
docker build -t registry.example.com/adatp/adatp-server:1.0.0 .
docker push registry.example.com/adatp/adatp-server:1.0.0

# 2. Set a real admin token (edit secret.yaml, or create it out of band):
kubectl create secret generic adatp-secrets \
  --from-literal=ADMIN_TOKEN="$(openssl rand -hex 32)" \
  --dry-run=client -o yaml | kubectl apply -f -

# 3. Apply everything:
kubectl apply -f deploy/kubernetes/
```

The Deployment wires up exactly what the task of running AdaTP needs:

- `containerPort: 3000`
- **liveness probe → `GET /healthz`** (no auth, no DB)
- **readiness probe → `GET /readyz`** (503 while draining or if SQLite is
  unreachable, so the Service stops routing during rollouts)
- a `startupProbe` so a slow first boot is not killed by liveness
- CPU/memory **requests and limits** (tune for your load — see
  [sizing.md](./sizing.md))
- env from the ConfigMap + Secret
- the SQLite PVC mounted at `/app/data`
- a non-root, `drop: [ALL]`, no-privilege-escalation security context (the
  image already runs as the `adatp` user)

> The ConfigMap contains only variables the server reads (`server/src/config.rs`
> / `server/.env.example`): `HOST`, `PORT`, `RUST_LOG`, `AUTH_DRIVER`,
> `AUTH_FILE_PATH`/`AUTH_API_URL`, `DATABASE_URL`, `MAX_FRAME_BYTES`,
> `IDLE_TIMEOUT_SECS`, `MAX_CONNECTIONS`, `MSG_RATE_LIMIT`. Switch to your IdP by
> setting `AUTH_DRIVER: api` and `AUTH_API_URL` ([auth-providers.md](./auth-providers.md)).

## TLS terminates outside the cluster

The Service is plain HTTP/WebSocket on port 3000 — **the server does not
terminate TLS** (see the TLS NOTE in `deployment.yaml`). Put an Ingress /
gateway / mesh in front of the Service to terminate `wss://` and forward to it.
TLS is REQUIRED in production ([security-hardening.md](./security-hardening.md)).
The Ingress MUST:

- allow the **WebSocket upgrade**, and
- keep long-lived connections open (raise idle/read timeouts above the server's
  ping interval).

Point the Ingress backend at `svc/adatp-server:3000`; terminate TLS there (see
[tls-cloudflare.md](./tls-cloudflare.md)).

## Draining before rollouts

The image contains no curl, so a `preStop` exec cannot call the admin API from
inside the container. Drain from your rollout pipeline instead, using the
`x-admin-token` header:

```bash
curl -X POST https://adatp.example.com/admin/v1/drain \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"enabled":true}'
# readiness flips to 503 → the Service stops routing new connections;
# watch active connections fall:
curl -s -H "x-admin-token: $ADMIN_TOKEN" https://adatp.example.com/admin/v1/overview
# then:
kubectl rollout restart deploy/adatp-server
```

(If you prefer an in-cluster preStop hook, build a derived image with curl.)

## Verify

```bash
kubectl get pods -l app.kubernetes.io/name=adatp-server
kubectl logs deploy/adatp-server | tail
kubectl port-forward svc/adatp-server 3000:3000 &
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

## Known v1 gaps on Kubernetes (stated honestly)

- **Single replica only** — no clustering; rooms are in-memory ([ha.md](./ha.md)).
- **No Prometheus endpoint** for a `ServiceMonitor` to scrape — `/api/metrics`
  is JSON behind `x-api-key`; a Prometheus/OTel exporter is roadmap
  ([observability.md](./observability.md)).
- Capacity numbers are **not published**; measure your own with
  [benchmarks.md](./benchmarks.md).
