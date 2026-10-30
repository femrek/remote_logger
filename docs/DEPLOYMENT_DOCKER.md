# Docker Compose Deployment Guide

This guide covers deploying the full **Remote Logger** pipeline using Docker and Docker Compose on a single virtual machine (VM), dedicated server, or local environment.

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Directory Structure](#directory-structure)
- [Step 1 — Configure Environment and Secrets](#step-1--configure-environment-and-secrets)
- [Step 2 — Configure Ports and Compose Overrides](#step-2--configure-ports-and-compose-overrides)
- [Step 3 — Start the Stack](#step-3--start-the-stack)
- [Step 4 — Verify Component Health](#step-4--verify-component-health)
- [Step 5 — Configure Channels in Consul KV](#step-5--configure-channels-in-consul-kv)
- [Step 6 — Verify Ingestion](#step-6--verify-ingestion)
- [Step 7 — Access Monitoring & Management UIs](#step-7--access-monitoring--management-uis)
- [Production Hardening & TLS](#production-hardening--tls)
- [Maintenance & Teardown](#maintenance--teardown)
- [Troubleshooting](#troubleshooting)

---

## Architecture Overview

In a Docker deployment, all services communicate over a private bridge network (`logger_network`), while external ingress is routed through Pingora (`logger_gateway`):

```
Mobile App / Ingestion Client
             │
             │ HTTPS / HTTP (:8080)
             ▼
   ┌────────────────────────────────────────────────────────┐
   │                     logger_gateway                     │
   │               (Cloudflare Pingora Proxy)               │
   │   • ES256 session token verification (public.pem)      │
   │   • Consul catalog dynamic service discovery           │
   │   • Routes auth vs ingestion traffic                   │
   └─────────────┬───────────────────────────┬──────────────┘
                 │                           │
  /auth/* routes │                           │ /v1/logs/* routes
                 ▼                           ▼
     ┌───────────────────────┐   ┌──────────────────────────┐
     │      auth_worker      │   │    validation_worker     │
     │   (Axum, 1 replica)   │   │    (Axum, 2 replicas)    │
     │ • Hardware attestation│   │ • Schema validation      │
     │ • Mint ES256 tokens   │   │ • Enriches payloads      │
     │ • Writes device-meta  │   │ • Publishes to Kafka     │
     └───────────┬───────────┘   └───────────┬──────────────┘
                 │                           │
                 │   Kafka protocol (:9094)  │
                 └─────────────┬─────────────┘
                               ▼
     ┌──────────────────────────────────────────────────────┐
     │                        broker                        │
     │             (Apache Kafka in KRaft mode)             │
     │           Topics: mobile-logs, device-metadata       │
     └─────────────────────────┬────────────────────────────┘
                               │
                               ▼
     ┌──────────────────────────────────────────────────────┐
     │                      fluent-bit                      │
     │              (Kafka consumer / forwarder)            │
     └─────────────────────────┬────────────────────────────┘
                               │ Bulk POST (:9200)
                               ▼
     ┌──────────────────────────────────────────────────────┐
     │                      opensearch                      │
     │              (Distributed search index)              │
     └──────────────────────────────────────────────────────┘

  Supporting Control Plane & Observability:
  • consul: Service registration and dynamic KV routing store (:8500)
  • logger_admin: Management UI and CAS API for Consul KV (:8082)
  • opensearch-dashboards: Search & analytics web interface (:5601)
  • kafka-ui: Topic and broker inspection web interface (:8081)
  • prometheus & grafana: Metric collection (:9090) and dashboards (:3000)
```

---

## Prerequisites

- **Docker Engine**: Version 24.0 or newer
- **Docker Compose**: Version 2.0 or newer (`docker compose` CLI plugin)
- **OpenSSL**: For generating ECDSA P-256 JWT keys and secret tokens
- **Hardware Resources**:
  - Minimum: 2 CPU cores, 4 GB RAM
  - Recommended for production: 4+ CPU cores, 8+ GB RAM (adjust OpenSearch heap size accordingly)

---

## Directory Structure

All Docker resources are housed in the `docker/` directory:

```
docker/
├── .env.template                     # Template environment variables
├── .env                              # Active environment configuration (gitignored)
├── docker-compose.yaml               # Core services and network definitions
├── docker-compose-network.yaml       # Port mapping and external network overrides
├── docker-compose-dev.yaml           # Local build-from-source overrides
├── docker-compose-override.yaml.template  # Customizable override template
├── fluent-bit.conf                   # Fluent Bit Kafka-to-OpenSearch pipeline
├── keys/                             # Cryptographic signing keys
│   ├── private.pem                   # ES256 private key for token minting (auth_worker)
│   └── public.pem                    # ES256 public key for token verification (gateway)
└── monitoring/
    ├── grafana/                      # Pre-provisioned Grafana datasources and dashboards
    └── prometheus.yml                # Prometheus scrape targets
```

---

## Step 1 — Configure Environment and Secrets

1. Navigate to the `docker/` folder and copy the environment template:
   ```bash
   cd docker
   cp .env.template .env
   ```

2. Generate a secure Bearer token for `logger_admin`:
   ```bash
   openssl rand -hex 32
   ```
   Set this value as `GATEWAY_ADMIN_TOKEN` in `docker/.env`.

3. Generate secure passwords for Grafana and OpenSearch:
   ```bash
   # OpenSearch password requires: 8+ chars, 1 uppercase, 1 lowercase, 1 digit, 1 symbol
   openssl rand -base64 16
   ```
   Set `GRAFANA_ADMIN_PASSWORD` and `OPENSEARCH_ADMIN_PASSWORD` in `docker/.env`.

4. Generate production ECDSA P-256 key pair for JWT signing (or use development keys in `keys/`):
   ```bash
   openssl ecparam -name prime256v1 -genkey -noout -out keys/private.pem
   openssl ec -in keys/private.pem -pubout -out keys/public.pem
   chmod 600 keys/private.pem
   ```

---

## Step 2 — Configure Ports and Compose Overrides

By default, `docker-compose.yaml` defines internal service communication across the `logger_network` bridge. To expose service ports to your host machine:

### Option A: Using `docker-compose-network.yaml` (Recommended)

In `docker/.env`, ensure `COMPOSE_FILE` includes `docker-compose-network.yaml`:

```ini
COMPOSE_FILE=docker-compose.yaml,docker-compose-network.yaml
COMPOSE_PATH_SEPARATOR=,
COMPOSE_PROJECT_NAME=logger_backend
```

This maps:
- `8080:8080` -> `logger_gateway` (Ingestion and Auth API)
- `8082:8082` -> `logger_admin` (Admin UI & REST API)
- `8500:8500` -> `consul` (Consul UI & HTTP API)
- `5601:5601` -> `opensearch-dashboards`
- `8081:8080` -> `kafka-ui` (mapped to 8081 to avoid conflict with gateway)
- `3000:3000` -> `grafana`
- `9090:9090` -> `prometheus`

### Option B: Custom Override

Copy `docker-compose-override.yaml.template` to `docker-compose-override.yaml` to customize port mappings or bind to existing reverse proxies (Nginx, Traefik, or Cloudflare Tunnels).

---

## Step 3 — Start the Stack

Pull pre-built images and start all containers in detached mode:

```bash
cd docker
docker compose up -d
```

To build images directly from local source instead of pulling from GHCR:
```bash
docker compose -f docker-compose.yaml -f docker-compose-dev.yaml up -d --build
```

---

## Step 4 — Verify Component Health

Check container status and ensure all services are healthy:

```bash
docker compose ps
```

Expected status:
- `logger_consul`: healthy
- `logger_broker`: running
- `logger_gateway`: running
- `auth_worker`: running
- `validation_worker` (2 replicas): running
- `logger_fluentbit`: running
- `logger_opensearch`: running

To inspect logs of any service:
```bash
docker compose logs -f logger_gateway
docker compose logs -f auth_worker
docker compose logs -f validation_worker
```

---

## Step 5 — Configure Channels in Consul KV

When `logger_admin` boots up against a clean Consul instance, it automatically seeds default configuration in Consul KV:
- **`gateway/config/global`**: Global limits (`max_body_bytes`, `jwt.issuer`, `jwt.ttl_seconds`).
- **`gateway/config/channels/example-app-dev`**: Development channel with attestation set to `off` (allows simulators and tests).
- **`gateway/config/channels/example-app-prod`**: Production channel with attestation set to `enforce`.

### Managing via Admin UI

1. Open `http://localhost:8082` in your browser.
2. Enter your `GATEWAY_ADMIN_TOKEN`.
3. Create, edit, or delete application channels (routing slugs, attestation enforcement modes, Apple Team IDs, and signing cert digests).

Any update committed through `logger_admin` is written atomically with Consul Check-And-Set (CAS) and hot-reloaded across `logger_gateway` and `auth_worker` instantly with **zero downtime**.

---

## Step 6 — Verify Ingestion

Send a test log event to verify end-to-end processing:

```bash
# From repository root
bash scripts/send_logs.sh
```

Send 10 bulk log events:
```bash
bash scripts/send_logs.sh --bulk 10
```

Verify that the metrics endpoints are actively collecting telemetry:
```bash
bash scripts/verify_metrics.sh
```

---

## Step 7 — Access Monitoring & Management UIs

| Component | Host URL | Authentication | Purpose |
|---|---|---|---|
| **Logger Admin UI** | `http://localhost:8082` | `GATEWAY_ADMIN_TOKEN` | Manage channels, attestation policies, and global settings |
| **Consul UI** | `http://localhost:8500` | None (by default) | Check service registry status, health checks, and KV tree |
| **OpenSearch Dashboards** | `http://localhost:5601` | `admin` / `OPENSEARCH_ADMIN_PASSWORD` | Discover, filter, and visualize indexed mobile logs |
| **Kafka UI** | `http://localhost:8081` | None | Inspect `mobile-logs` and `device-metadata` topics |
| **Grafana Dashboards** | `http://localhost:3000` | `admin` / `GRAFANA_ADMIN_PASSWORD` | Real-time dashboards: request latency, ingestion rate, error counts |
| **Prometheus Targets** | `http://localhost:9090/targets` | None | Verify scrape status of all workers, gateway, and exporters |

---

## Production Hardening & TLS

### 1. TLS Termination at Pingora Gateway
To terminate TLS directly at `logger_gateway`:
1. Place SSL certificates on the host (e.g. `/etc/certs/server.crt` and `/etc/certs/server.key`).
2. Mount the certificates into the `logger_gateway` container in your compose override.
3. Configure environment variables in `docker/.env`:
   ```ini
   GATEWAY_TLS_CERT_PATH=/etc/certs/server.crt
   GATEWAY_TLS_KEY_PATH=/etc/certs/server.key
   ```

### 2. Upstream Reverse Proxy / Cloudflare Tunnel
If using an external ingress proxy (e.g. Cloudflare Tunnel, Traefik, Nginx):
- Route incoming HTTPS traffic to `logger_gateway:8080`.
- Keep management ports (`8082`, `8500`, `9090`, `5601`) behind VPN or internal firewall.

### 3. Persistent Volumes
Docker Compose persists stateful data across restarts using named volumes:
- `consul-data`: Channel configurations and KV store.
- `opensearch-data`: Indexed logs and mappings.
- `prometheus-data`: Time-series metric data.
- `grafana-data`: Dashboards and alerts.

Ensure these volumes are included in regular backup routines.

---

## Maintenance & Teardown

### Stopping Services
```bash
cd docker
docker compose stop
```

### Stopping and Removing Containers
```bash
cd docker
docker compose down
```

### Updating Images
```bash
cd docker
docker compose pull
docker compose up -d
```

---

## Troubleshooting

- **502 Bad Gateway from `logger_gateway`:**
  Ensure workers are healthy and registered with Consul. Check `http://localhost:8500/v1/catalog/services` to confirm `validation_worker` and `auth_worker` are registered.
- **`auth_worker` JWT minting error:**
  Verify `keys/private.pem` exists, has correct read permissions (`chmod 600`), and begins with `-----BEGIN PRIVATE KEY-----` or `-----BEGIN EC PRIVATE KEY-----`.
- **OpenSearch fails to start (exit code 137 or 1):**
  Check system virtual memory limits:
  ```bash
  sudo sysctl -w vm.max_map_count=262144
  ```
- **Fluent Bit connection errors:**
  Ensure `opensearch` has finished starting before checking Fluent Bit logs:
  ```bash
  docker compose logs -f logger_fluentbit
  ```
