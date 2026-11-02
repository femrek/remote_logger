# Kubernetes Deployment Guide

> **OUTDATED**: This Kubernetes deployment guide and the manifests in this directory are currently outdated and may not reflect recent architecture updates (such as Consul KV dynamic configuration and two-tier authentication). For the actively supported deployment workflow, please refer to the [Docker Compose Deployment Guide](../docs/DEPLOYMENT_DOCKER.md).

This guide covers how to deploy the **Remote Logger** pipeline on any Kubernetes cluster — including local clusters (Minikube, Kind, Docker Desktop) and cloud providers (GKE, EKS, AKS).

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Folder Structure](#folder-structure)
- [Step 1 — Prepare Secrets & Environment Files](#step-1--prepare-secrets--environment-files)
- [Step 2 — Configure Routing & Consul Channels](#step-2--configure-routing--consul-channels)
- [Step 3 — Deploy the Stack](#step-3--deploy-the-stack)
- [Step 4 — Verify the Deployment](#step-4--verify-the-deployment)
- [Step 5 — Access the Services](#step-5--access-the-services)
- [Step 6 — Send a Test Log](#step-6--send-a-test-log)
- [Scaling Validation Workers](#scaling-validation-workers)
- [Persistent Storage (Production)](#persistent-storage-production)
- [Tearing Down](#tearing-down)
- [Troubleshooting](#troubleshooting)

---

## Architecture Overview

```
Internet / Mobile Clients
        │  HTTP :8080
        ▼
  logger-gateway            (LoadBalancer / NodePort :30080)
   Pingora Proxy            • ES256 JWT signature verification
                            • Discovers workers via Consul catalog
                            • Routes auth vs ingest traffic
        │
        ├── /auth/{challenge,enroll,exchange} (Proxied to auth-worker)
        │   ▼
        │ auth-worker              (Deployment, 2 replicas)
        │  Axum microservice       • Android Key Attestation / Apple App Attest
        │                          • ES256 enrollment & session JWT minting (private.pem)
        │                          • Startup metadata published to Kafka
        │
        └── POST /v1/logs/:channel (Proxied to validation-worker)
            ▼
      validation-worker         (Deployment, 2 replicas)
       Axum microservice        • Schema validation & enrichments
                                • Produces to Kafka with broker ACK
        │
        ▼
  broker                    (StatefulSet, KRaft mode)
   Apache Kafka             • Distributed high-throughput log bus
        │
        ▼
  fluent-bit                (Deployment)
   Log forwarder            • Consumes Kafka topic → bulk POSTs to OpenSearch
        │
        ▼
  opensearch                (StatefulSet)
   Search & Analytics       • Indexes documents (:9200)
        │
  opensearch-dashboards     (Deployment, :5601)
  consul                    (Deployment, :8500)
  kafka-ui                  (Deployment, NodePort :30081)
```

---

## Prerequisites

| Tool | Minimum Version | Notes |
|---|---|---|
| `kubectl` | 1.25+ | [Kubernetes CLI install guide](https://kubernetes.io/docs/tasks/tools/) |
| A running Kubernetes cluster | 1.25+ | Minikube, Kind, Docker Desktop, GKE, EKS, AKS |
| `kustomize` | Bundled in `kubectl` | Used via `kubectl apply -k` |

> [!NOTE]
> OpenSearch requires sufficient memory. Allocate **at least 4 GB RAM** to your cluster node when running locally.

---

## Folder Structure

```
kubernetes/
├── kustomization.yaml              ← Entry point for kubectl apply -k
├── namespace.yaml                  ← Namespace definition (logger-backend)
├── config/
│   ├── configmaps.yaml             ← ConfigMaps for gateway and fluent-bit
│   ├── secrets.env.example         ← Environment template for logger-secrets
│   └── docker-config.json.example  ← Template for image pull secrets (GHCR)
├── consul/
│   └── consul.yaml                 ← Consul agent deployment & service
├── broker/
│   └── kafka.yaml                  ← Apache Kafka KRaft cluster
├── opensearch/
│   ├── opensearch.yaml             ← OpenSearch & Dashboards manifests
│   ├── opensearch.env.example      ← OpenSearch credentials template
│   └── htpasswd.example            ← OpenSearch Dashboards HTTP basic auth template
├── fluent-bit/
│   └── fluent-bit.yaml             ← Fluent Bit forwarder deployment
├── auth-worker/
│   └── auth-worker.yaml            ← Axum authentication & attestation worker deployment & service
├── validation-worker/
│   └── validation-worker.yaml      ← Axum validation worker deployment & service
├── gateway/
│   └── logger-gateway.yaml         ← Pingora proxy deployment & LoadBalancer service
├── kafka-ui/
│   └── kafka-ui.yaml               ← Kafka web UI deployment
├── monitoring/                     ← Prometheus, Grafana, exporters manifests
│   ├── prometheus/
│   ├── grafana/
│   ├── kafka-exporter/
│   ├── opensearch-exporter/
│   └── consul-exporter/
└── DEPLOYMENT.md
```

---

## Step 1 — Prepare Secrets & Environment Files

Kustomize generates the required Secrets using environment template files. Before deploying, copy each `.example` file to its active filename and fill in values:

### 1. Application Secrets (`config/secrets.env`)
```bash
cp kubernetes/config/secrets.env.example kubernetes/config/secrets.env
```
Generate an Admin Token for `logger_admin` and set `GATEWAY_ADMIN_TOKEN`.

### 2. OpenSearch Credentials
```bash
cp kubernetes/opensearch/opensearch.env.example kubernetes/opensearch/opensearch.env
cp kubernetes/monitoring/opensearch.env.example kubernetes/monitoring/opensearch.env
cp kubernetes/opensearch/htpasswd.example kubernetes/opensearch/htpasswd
```
Set strong passwords for OpenSearch cluster access and Grafana dashboards.

### 3. Container Registry Pull Secret (Optional)
If pulling private images from GitHub Container Registry (GHCR):
```bash
cp kubernetes/config/docker-config.json.example kubernetes/config/docker-config.json
# Fill in your base64-encoded docker credentials
```

### 4. ECDSA P-256 Key Pair (JWT Authentication)
The pipeline uses **ES256 (ECDSA P-256)** for enrollment and session JWT minting and verification. Generate a production key pair:
```bash
openssl ecparam -name prime256v1 -genkey -noout -out private.pem
openssl ec -in private.pem -pubout -out public.pem
```
Mount `private.pem` into the `auth-worker` pod (via the `logger-jwt-keys` secret, configured at `GATEWAY_PRIVATE_KEY_PATH=/etc/logger/keys/private.pem`) and `public.pem` into the `logger-gateway` pod (configured via `GATEWAY_PUBLIC_KEY_PATH=/etc/logger/keys/public.pem`).

---

## Step 2 — Configure Routing & Consul Channels

Dynamic channel routing and attestation rules are managed through **Consul KV** at `gateway/config/channels/<channel-id>`.

When Consul starts, you can register channels via the `logger_admin` API or direct Consul HTTP API calls:

```bash
# Example: Seed a development channel via Consul HTTP API
curl -X PUT "http://consul.logger-backend.svc.cluster.local:8500/v1/kv/gateway/config/channels/example-app-dev" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "example-app-dev",
    "app_id": "dev.femrek.exampleapp.dev",
    "android_attestation_mode": "off",
    "ios_attestation_mode": "off"
  }'
```

`logger-gateway` (watching `gateway/config/global`) and `auth-worker` (watching `gateway/config/channels/`) use Consul HTTP blocking queries and hot-reload changes without pod restarts.

---

## Step 3 — Deploy the Stack

Deploy all resources in a single command using Kustomize from the repository root:

```bash
kubectl apply -k kubernetes/
```

### Check Rollout Status

```bash
kubectl rollout status statefulset/broker            -n logger-backend
kubectl rollout status deployment/consul             -n logger-backend
kubectl rollout status statefulset/opensearch        -n logger-backend
kubectl rollout status deployment/auth-worker        -n logger-backend
kubectl rollout status deployment/validation-worker  -n logger-backend
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
auth-worker-xxxxxxxx-xxxxx            1/1     Running   0          2m
auth-worker-xxxxxxxx-xxxxx            1/1     Running   0          2m
broker-0                              1/1     Running   0          2m
consul-xxxxxxxxx-xxxxx                1/1     Running   0          2m
fluent-bit-xxxxxxxxx-xxxxx            1/1     Running   0          2m
kafka-ui-xxxxxxxxx-xxxxx              1/1     Running   0          2m
logger-gateway-xxxxxxxxx-xxxxx        1/1     Running   0          2m
opensearch-0                          1/1     Running   0          2m
opensearch-dashboards-xxxxxxxx-xxxxx  1/1     Running   0          2m
validation-worker-xxxxxxxx-xxxxx      1/1     Running   0          2m
validation-worker-xxxxxxxx-xxxxx      1/1     Running   0          2m
```

---

## Step 5 — Access the Services

### Logger Gateway (Ingest Endpoint)
The gateway is exposed via a `LoadBalancer` Service on port `8080` (NodePort `30080` on local clusters):

- **Cloud (GKE/EKS/AKS):** Use external IP from `kubectl get svc logger-gateway -n logger-backend`.
- **Minikube:** Access via NodePort: `http://$(minikube ip):30080`.
- **Docker Desktop / Kind:** `http://localhost:30080` or `http://localhost:8080`.

### Cluster UIs (via Port-Forward)

| Service | Command | Local URL |
|---|---|---|
| OpenSearch Dashboards | `kubectl port-forward svc/opensearch-dashboards 5601:5601 -n logger-backend` | http://localhost:5601 |
| Consul UI | `kubectl port-forward svc/consul 8500:8500 -n logger-backend` | http://localhost:8500 |
| Kafka UI | NodePort `30081` or port-forward | http://localhost:30081 |
| Grafana | `kubectl port-forward svc/grafana 3000:3000 -n monitoring` | http://localhost:3000 |
| Prometheus | `kubectl port-forward svc/prometheus 9090:9090 -n monitoring` | http://localhost:9090 |

---

## Step 6 — Send a Test Log

### 1. Exchange Hardware Attestation for a Session JWT

Send the attestation exchange request to `/auth/exchange` with the required `X-Channel-Id` header:

```bash
GATEWAY_HOST="http://localhost:30080"  # Adjust for your cluster endpoint

curl -s -X POST "$GATEWAY_HOST/auth/exchange" \
  -H "Content-Type: application/json" \
  -H "X-Channel-Id: example-app-dev" \
  -d '{
    "platform": "android",
    "attestation_token": "mock-token",
    "device_id": "test-device-001",
    "device_metadata": {
      "timestamp": "2026-10-02T12:00:00Z",
      "deviceId": "test-device-001",
      "sessionId": "session-k8s-001",
      "os": "Android",
      "osVersion": "14",
      "model": "K8s-Test",
      "brand": "Cluster",
      "device": "node",
      "identifier": "test-build-1",
      "manufacturer": "Cluster",
      "isPhysicalDevice": "false"
    }
  }' | tee /tmp/auth_response.json
```

Extract the minted access token:
```bash
ACCESS_TOKEN=$(jq -r '.access_token' /tmp/auth_response.json)
```

### 2. Post a Log Record

```bash
curl -s -X POST "$GATEWAY_HOST/v1/logs/example-app-dev" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "timestamp":  "2026-10-02T12:00:00Z",
    "loggerName": "KubernetesTester",
    "message":    "Hello from Kubernetes deployment!",
    "level":      "INFO",
    "zone":       ""
  }'
```

A `200 OK` response confirms the log was received by the gateway, validated by the worker, and acknowledged by Kafka.

### 3. Verify in OpenSearch

```bash
kubectl port-forward svc/opensearch 9200:9200 -n logger-backend &
sleep 2

curl "http://localhost:9200/mobile-logs-*/_search?q=message:Kubernetes&pretty"
```

---

## Scaling Validation Workers

Validation workers register dynamically in Consul. To scale the worker deployment:

```bash
kubectl scale deployment/validation-worker --replicas=4 -n logger-backend
```

As new worker pods become ready, they register with Consul and `logger-gateway` automatically adds them to its Pingora round-robin upstream pool. No configuration changes or gateway restarts are needed.

---

## Persistent Storage (Production)

For production, configure `PersistentVolumeClaims` with dynamic StorageClasses:

- **Kafka:** In `kubernetes/broker/kafka.yaml`, uncomment `volumeClaimTemplates` for `kafka-storage`.
- **OpenSearch:** In `kubernetes/opensearch/opensearch.yaml`, uncomment `volumeClaimTemplates` for `opensearch-data`.

---

## Tearing Down

To remove all resources and data from the cluster:

```bash
kubectl delete -k kubernetes/
```

To remove only workloads while retaining the namespace:
```bash
kubectl delete namespace logger-backend monitoring
```

---

## Troubleshooting

### `502 Bad Gateway` on Ingest Requests
- Verify that `consul` is healthy: `kubectl logs deployment/consul -n logger-backend`.
- Verify that `validation-worker` pods are registered in Consul:
  ```bash
  kubectl exec deployment/logger-gateway -n logger-backend -- \
    curl -s http://consul.logger-backend.svc.cluster.local:8500/v1/health/service/validation_worker?passing=true
  ```

### Workers Failing to Mint Tokens
- Confirm that ECDSA P-256 keys are mounted. Workers look for `private.pem` via `GATEWAY_PRIVATE_KEY_PATH`.
