# Kubernetes Deployment Guide

This guide covers how to deploy the **Remote Logger Backend** on any Kubernetes cluster — including local development clusters (Minikube, Kind, Docker Desktop) and cloud providers (GKE, EKS, AKS).

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Folder Structure](#folder-structure)
- [Step 1 — Set the JWT Secret](#step-1--set-the-jwt-secret)
- [Step 2 — Configure Your Channels](#step-2--configure-your-channels)
- [Step 3 — Deploy the Stack](#step-3--deploy-the-stack)
- [Step 4 — Verify the Deployment](#step-4--verify-the-deployment)
- [Step 5 — Access the Services](#step-5--access-the-services)
- [Step 6 — Send a Test Log](#step-6--send-a-test-log)
- [Scaling the Validation Worker](#scaling-the-validation-worker)
- [Persistent Storage (Production)](#persistent-storage-production)
- [Tearing Down](#tearing-down)
- [Troubleshooting](#troubleshooting)

---

## Architecture Overview

```
Internet / Mobile Client
        │  HTTP :8080
        ▼
  logger-gateway            (LoadBalancer)
   Pingora Proxy            - JWT verification
                            - Per-channel routing
                            - Load balances across workers
        │
        ▼
  validation-worker-{0,1}   (StatefulSet, 2 replicas)
   Axum microservice        - Schema validation
                            - JWT minting (/auth/exchange)
                            - Kafka producer (zero data loss)
        │
        ▼
  broker                    (StatefulSet, KRaft mode)
   Apache Kafka             - Message bus
        │
        ▼
  fluent-bit                (Deployment)
   Log forwarder            - Consumes Kafka → bulk POSTs to OpenSearch
        │
        ▼
  opensearch                (StatefulSet)
   Search & Analytics       :9200
        │
  opensearch-dashboards     (Deployment)   :5601
  kafka-ui                  (Deployment)   :30081
```

---

## Prerequisites

| Tool | Minimum version | Notes |
|---|---|---|
| `kubectl` | 1.25 | [Install guide](https://kubernetes.io/docs/tasks/tools/) |
| A running Kubernetes cluster | 1.25 | Minikube, Kind, Docker Desktop, GKE, EKS, AKS |
| `kustomize` | bundled in `kubectl` v1.14+ | Used via `kubectl apply -k` |

> **Local cluster options:**
> - **Docker Desktop** — Enable Kubernetes in _Settings → Kubernetes_.
> - **Minikube** — `minikube start --memory=4096`
> - **Kind** — `kind create cluster`
>
> OpenSearch is memory-hungry. Allocate **at least 4 GB RAM** to your local cluster node.

---

## Folder Structure

```
kubernetes/
├── kustomization.yaml              ← entry point for kubectl apply -k
├── namespace.yaml
├── config/
│   ├── secrets.yaml                ← GATEWAY_JWT_SECRET (edit before deploying)
│   └── configmaps.yaml             ← gateway.yaml, workers.txt, channels.yaml, fluent-bit.conf
├── broker/
│   └── kafka.yaml
├── opensearch/
│   └── opensearch.yaml
├── fluent-bit/
│   └── fluent-bit.yaml
├── validation-worker/
│   └── validation-worker.yaml
├── gateway/
│   └── logger-gateway.yaml
└── kafka-ui/
    └── kafka-ui.yaml
```

---

## Step 1 — Set the JWT Secret

The `GATEWAY_JWT_SECRET` is the HMAC key shared between the gateway (which **validates** ingest JWTs) and the validation worker (which **mints** them). It must be set before deploying.

> [!CAUTION]
> The file `kubernetes/config/secrets.yaml` contains a placeholder secret. **Never deploy the placeholder to a real environment.** Replace it with a strong random value.

### Option A — Edit the file directly (simple, dev-only)

Generate a random secret and base64-encode it:

```bash
echo -n "$(openssl rand -hex 32)" | base64
```

Open `kubernetes/config/secrets.yaml` and replace the value:

```yaml
data:
  GATEWAY_JWT_SECRET: <paste-your-base64-value-here>
```

### Option B — Create the secret with kubectl (recommended for all environments)

Skip editing `secrets.yaml` and instead use `kubectl` directly. First, **remove** `config/secrets.yaml` from `kubernetes/kustomization.yaml`, then run:

```bash
kubectl create secret generic logger-secrets \
  --from-literal=GATEWAY_JWT_SECRET="$(openssl rand -hex 32)" \
  --namespace logger-backend
```

> [!NOTE]
> If you choose Option B, run `kubectl create namespace logger-backend` first (or apply only `namespace.yaml`) before creating the secret.

---

## Step 2 — Configure Your Channels

A **channel** maps a URL path to an application. Each channel has its own app identity and attestation settings.

Edit the two channel config blocks inside `kubernetes/config/configmaps.yaml`.

### `gateway-config` → `gateway.yaml` section

This controls routing at the gateway level:

```yaml
channels:
  - id: "my-app-prod"
    path_prefix: "/v1/logs/my-app-prod"    # URL prefix clients POST to
    app_id: "com.example.myapp"
    headers:
      X-Channel-Id: "my-app-prod"
      X-Environment: "production"
      X-Kafka-Topic: "mobile-logs-prod"    # Kafka topic logs land in
```

### `validation-worker-config` → `channels.yaml` section

This controls attestation policy per channel:

```yaml
channels:
  - id: "my-app-prod"
    app_id: "com.example.myapp"
    validate_attestation: true           # Enforce Play Integrity / App Attest
    google_project_number: "YOUR_GCP_PROJECT_NUMBER"
    apple_team_id: "YOUR_APPLE_TEAM_ID"
    require_apple_key_hash: true

  - id: "my-app-dev"
    app_id: "com.example.myapp.dev"
    validate_attestation: false          # Safe to disable in development/CI
    require_apple_key_hash: false
```

> [!IMPORTANT]
> The `id` field must match between `gateway.yaml` and `channels.yaml` for the same logical channel.

### `gateway-config` → `workers.txt` section

This file tells the gateway where to find the validation worker pods. By default it has 2 entries matching the StatefulSet pod FQDNs:

```
validation-worker-0.validation-worker-service.logger-backend.svc.cluster.local:8000
validation-worker-1.validation-worker-service.logger-backend.svc.cluster.local:8000
```

If you change the replica count (see [Scaling](#scaling-the-validation-worker)), add or remove lines here to match.

---

## Step 3 — Deploy the Stack

Apply all resources with a single command from the repository root:

```bash
kubectl apply -k kubernetes/
```

This creates the namespace, secrets, ConfigMaps, and all workloads in the correct order.

### Check rollout status

```bash
kubectl rollout status statefulset/broker            -n logger-backend
kubectl rollout status statefulset/opensearch        -n logger-backend
kubectl rollout status statefulset/validation-worker -n logger-backend
kubectl rollout status deployment/fluent-bit         -n logger-backend
kubectl rollout status deployment/logger-gateway     -n logger-backend
```

---

## Step 4 — Verify the Deployment

List all pods and confirm every pod is `Running`:

```bash
kubectl get pods -n logger-backend
```

Expected output:

```
NAME                                  READY   STATUS    RESTARTS   AGE
broker-0                              1/1     Running   0          2m
fluent-bit-xxxxxxxxx-xxxxx            1/1     Running   0          2m
kafka-ui-xxxxxxxxx-xxxxx              1/1     Running   0          2m
logger-gateway-xxxxxxxxx-xxxxx        1/1     Running   0          2m
opensearch-0                          1/1     Running   0          2m
opensearch-dashboards-xxxxxxxx-xxxxx  1/1     Running   0          2m
validation-worker-0                   1/1     Running   0          2m
validation-worker-1                   1/1     Running   0          2m
```

> [!NOTE]
> OpenSearch takes ~60–90 seconds to become `Ready` on first start. The `fluent-bit` and `logger-gateway` pods may restart once while waiting for it — this is normal.

Check pod logs if something isn't starting:

```bash
kubectl logs -n logger-backend statefulset/broker
kubectl logs -n logger-backend deployment/logger-gateway
kubectl logs -n logger-backend statefulset/validation-worker
```

---

## Step 5 — Access the Services

### Logger Gateway (the ingest endpoint)

The gateway is exposed via a `LoadBalancer` service on port `8080`.

**On a cloud cluster**, get the external IP:

```bash
kubectl get svc logger-gateway -n logger-backend
# Look for the EXTERNAL-IP column. It may take a minute to provision.
```

**On a local cluster (Minikube)**, use the NodePort `30080`:

```bash
# Minikube
minikube service logger-gateway -n logger-backend

# Or access directly via NodePort:
curl http://$(minikube ip):30080/
```

**On Docker Desktop**, `localhost:30080` works out of the box.

### Internal UIs (via port-forward)

The following services are cluster-internal. Use `kubectl port-forward` to access them locally:

| Service | Command | Local URL |
|---|---|---|
| OpenSearch Dashboards | `kubectl port-forward svc/opensearch-dashboards 5601:5601 -n logger-backend` | http://localhost:5601 |
| Kafka UI | Accessible via NodePort | http://localhost:30081 |
| OpenSearch REST API | `kubectl port-forward svc/opensearch 9200:9200 -n logger-backend` | http://localhost:9200 |
| Gateway Prometheus metrics | `kubectl port-forward svc/logger-gateway-metrics 9090:9090 -n logger-backend` | http://localhost:9090/metrics |

---

## Step 6 — Send a Test Log

### 1. Exchange an attestation token for a JWT

```bash
GATEWAY_HOST="http://localhost:30080"  # adjust for your cluster

curl -s -X POST "$GATEWAY_HOST/v1/logs/example-app-dev/auth/exchange" \
  -H "Content-Type: application/json" \
  -d '{
    "channel_id": "example-app-dev",
    "attestation_token": "dev-bypass-token",
    "device_uuid": "test-device-001"
  }' | tee /tmp/auth_response.json
```

Extract the access token:
```bash
ACCESS_TOKEN=$(jq -r '.access_token' /tmp/auth_response.json)
```

### 2. Post a log entry

```bash
curl -s -X POST "$GATEWAY_HOST/v1/logs/example-app-dev" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp":  "2026-01-01T12:00:00Z",
    "loggerName": "AppDatabase",
    "message":    "Hello from Kubernetes!",
    "level":      "INFO",
    "zone":       "",
    "deviceUUID": "test-device-001",
    "sessionId":  "session-k8s-test"
  }'
```

A `200 OK` response confirms the log was validated and acknowledged by Kafka.

### 3. Verify in OpenSearch

```bash
# After ~5 seconds (Fluent Bit flush interval)
kubectl port-forward svc/opensearch 9200:9200 -n logger-backend &

curl "http://localhost:9200/mobile-logs-*/_search?q=message:Kubernetes&pretty"
```

---

## Scaling the Validation Worker

To change the number of worker replicas, edit `kubernetes/validation-worker/validation-worker.yaml`:

```yaml
spec:
  replicas: 3   # change from 2 to desired count
```

Then add the new pod's FQDN to `workers.txt` in `kubernetes/config/configmaps.yaml`:

```
validation-worker-0.validation-worker-service.logger-backend.svc.cluster.local:8000
validation-worker-1.validation-worker-service.logger-backend.svc.cluster.local:8000
validation-worker-2.validation-worker-service.logger-backend.svc.cluster.local:8000
```

Re-apply to update the cluster:

```bash
kubectl apply -k kubernetes/
```

The gateway's file-watcher detects the ConfigMap update and reloads the worker list automatically (no gateway restart needed).

> [!NOTE]
> Kubernetes ConfigMap volume mounts are updated eventually — allow up to 60 seconds for the gateway to pick up new worker entries.

---

## Persistent Storage (Production)

By default, Kafka and OpenSearch use `emptyDir` volumes, meaning **data is lost when pods restart**. For production deployments, switch to PersistentVolumeClaims.

### Kafka

In `kubernetes/broker/kafka.yaml`, comment out the `emptyDir` volume and uncomment `volumeClaimTemplates`:

```yaml
  volumeClaimTemplates:
    - metadata:
        name: kafka-storage
      spec:
        accessModes: [ "ReadWriteOnce" ]
        resources:
          requests:
            storage: 10Gi
```

Also remove the corresponding entry from `spec.template.spec.volumes`.

### OpenSearch

In `kubernetes/opensearch/opensearch.yaml`, do the same:

```yaml
  volumeClaimTemplates:
    - metadata:
        name: opensearch-data
      spec:
        accessModes: [ "ReadWriteOnce" ]
        resources:
          requests:
            storage: 20Gi
```

> [!IMPORTANT]
> Your cluster must have a StorageClass that supports dynamic provisioning. Cloud providers (GKE, EKS, AKS) provide this by default. For Minikube, use `minikube addons enable storage-provisioner`.

---

## Tearing Down

Remove all resources created in the `logger-backend` namespace:

```bash
kubectl delete namespace logger-backend
```

This deletes all Pods, Services, ConfigMaps, Secrets, and PersistentVolumeClaims in the namespace.

To remove only the workloads while keeping the namespace:

```bash
kubectl delete -k kubernetes/
```

---

## Troubleshooting

### Pod stuck in `Pending`

```bash
kubectl describe pod <pod-name> -n logger-backend
```

Common causes:
- **Insufficient resources** — Your cluster node doesn't have enough CPU/memory. OpenSearch needs at least 2 GB RAM.
- **No StorageClass** — Only affects persistent volume setups. Verify with `kubectl get storageclass`.

### `validation-worker` pods not connecting to Kafka

Kafka starts slowly. The workers will retry automatically. Check Kafka is healthy first:

```bash
kubectl logs broker-0 -n logger-backend | tail -20
```

### Gateway returns `502 Bad Gateway`

The workers may not be ready yet, or the `workers.txt` ConfigMap has incorrect hostnames. Verify:

```bash
kubectl exec -n logger-backend deployment/logger-gateway -- cat /docker/workers.txt
```

The entries must exactly match:
```
validation-worker-0.validation-worker-service.logger-backend.svc.cluster.local:8000
validation-worker-1.validation-worker-service.logger-backend.svc.cluster.local:8000
```

### OpenSearch Dashboards shows "Server is not ready yet"

OpenSearch itself is still initialising. This message disappears once the `opensearch-0` pod passes its readiness check. Wait 1–2 minutes and refresh.

### Viewing all events in the namespace

```bash
kubectl get events -n logger-backend --sort-by='.lastTimestamp'
```
