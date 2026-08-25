# Install — Kubernetes (single replica)

**Read first:** AdaTP v1 keeps rooms and connections in process memory.
Kubernetes cannot change that:

- `replicas: 1`, `strategy: Recreate`. Two replicas would be two unrelated
  chat servers behind one Service — users would land in different "worlds".
- HPA is inappropriate; scaling out requires app-level sharding ([ha.md](./ha.md)).
- Session affinity is irrelevant (there is only one pod).
- A pod restart drops every session (clients reconnect and re-join).

## Starter manifests

```yaml
apiVersion: v1
kind: Secret
metadata: { name: adatp-secrets }
stringData:
  ADMIN_TOKEN: "<long-random-secret>"
---
apiVersion: v1
kind: ConfigMap
metadata: { name: adatp-config }
data:
  HOST: "0.0.0.0"
  PORT: "3000"
  AUTH_DRIVER: "api"
  AUTH_API_URL: "https://auth.internal.example.com/verify"
  DATABASE_URL: "sqlite:/app/data/adatp.db"
  RUST_LOG: "warn"
  MAX_CONNECTIONS: "10000"
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata: { name: adatp-data }
spec:
  accessModes: ["ReadWriteOnce"]
  resources: { requests: { storage: 1Gi } }
---
apiVersion: apps/v1
kind: Deployment
metadata: { name: adatp }
spec:
  replicas: 1                      # v1 is single-node — do not increase
  strategy: { type: Recreate }     # SQLite volume is RWO; no overlap
  selector: { matchLabels: { app: adatp } }
  template:
    metadata: { labels: { app: adatp } }
    spec:
      terminationGracePeriodSeconds: 30
      containers:
        - name: adatp
          image: registry.example.com/adatp/adatp-server:1.0.0
          # The server traps both SIGTERM and SIGINT (v1.0.0), so the
          # default K8s termination signal drains gracefully — no wrapper
          # needed; the image's default CMD is used as-is.
          ports: [{ containerPort: 3000 }]
          envFrom:
            - configMapRef: { name: adatp-config }
            - secretRef: { name: adatp-secrets }
          volumeMounts:
            - { name: data, mountPath: /app/data }
          livenessProbe:
            httpGet: { path: /healthz, port: 3000 }
            periodSeconds: 15
          readinessProbe:
            httpGet: { path: /readyz, port: 3000 }   # 503 while draining
            periodSeconds: 5
          resources:
            requests: { cpu: 500m, memory: 256Mi }
            limits: { memory: 1Gi }
      volumes:
        - name: data
          persistentVolumeClaim: { claimName: adatp-data }
---
apiVersion: v1
kind: Service
metadata: { name: adatp }
spec:
  selector: { app: adatp }
  ports: [{ port: 3000, targetPort: 3000 }]
```

Expose with your Ingress of choice — it MUST support WebSocket upgrade and
long-lived connections (raise any idle/read timeouts above the server's 30 s
ping interval). TLS at the Ingress; see [tls-cloudflare.md](./tls-cloudflare.md).

## Draining before rollouts

The image contains no curl, so a `preStop` exec cannot call the admin API
from inside the container. Drain from your rollout pipeline instead:

```bash
kubectl exec deploy/adatp -- true   # (no-op) — drain via the API from outside:
curl -X POST https://adatp.example.com/admin/v1/drain \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"enabled":true}'
# readiness flips 503 → the Service stops routing new connections;
# watch active connections fall:
curl -s -H "x-admin-token: $ADMIN_TOKEN" https://adatp.example.com/admin/v1/overview
# then:
kubectl rollout restart deploy/adatp
```

(If you prefer an in-cluster preStop hook, build a derived image with curl.)

## Verify

```bash
kubectl get pods -l app=adatp
kubectl logs deploy/adatp | tail
kubectl port-forward svc/adatp 3000:3000 &
cargo run -p adatp-cli -- -a 127.0.0.1:3000 -u user1 -p password123
```

Known v1 gaps on Kubernetes, stated honestly: no Prometheus endpoint for ServiceMonitor scraping
([observability.md](./observability.md)), single replica only.
