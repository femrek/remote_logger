# 🏛️ System Architecture

A detailed overview of the system architecture, component interactions, and data flows of **Remote Logger**.

The platform separates fast edge ingestion from search indexing using an event-driven, decoupled design. Mobile traffic is authenticated at the edge, buffered through Apache Kafka with zero data loss, and asynchronously indexed into OpenSearch for high-throughput querying.

---

## 📑 Table of Contents

- [Architecture Overview](#-architecture)
  - [Data Plane Pipeline](#data-plane-pipeline)
  - [Control & Configuration Plane](#control--configuration-plane)
- [Services at a Glance](#services-at-a-glance)
- [Project Structure](#project-structure)

---

## 📐 Architecture

### Data Plane Pipeline

The data plane processes all inbound authentication and log ingestion requests:
1. **`logger_gateway` (Pingora)** terminates client HTTP connections, validates ES256 session JWT signatures against `public.pem`, verifies channel path bindings, and dynamically routes traffic to `auth_worker` or `validation_worker` based on Consul service discovery.
2. **`auth_worker` (Axum)** performs hardware attestation (Google Play Integrity & Apple App Attest), mints short-lived access and long-lived refresh JWTs signed with `private.pem`, and publishes `SystemStartupEvent` device metadata to Kafka (`device-metadata`).
3. **`validation_worker` (Axum)** performs high-throughput `ServerLog` schema validation (single or batch), enriches payloads with gateway-injected identity headers (`x-app-id`, `x-device-uuid`, `x-session-id`), and publishes to Kafka (`mobile-logs`).
4. **Apache Kafka (KRaft)** acts as a resilient buffer topic (`mobile-logs` and `device-metadata`). Workers await broker ACKs before responding `200 OK`.
5. **Fluent Bit** consumes log streams from Kafka and bulk POSTs to OpenSearch every 2 seconds.
6. **OpenSearch & Dashboards** provides document indexing, search APIs, and Kibana-compatible visualization.

```
                       Mobile Clients (Dart / Flutter)
                                      │
                                      │  HTTP :8080
                                      ▼
                          ┌───────────────────────┐
                          │    logger_gateway     │
                          │    (Pingora Proxy)    │
                          └───┬───────────────┬───┘
          /auth/exchange      │               │  /v1/logs/:channel (JWT validated)
          (blind proxy)       ▼               ▼
        ┌─────────────────────────┐       ┌─────────────────────────┐
        │       auth_worker       │       │    validation_worker    │
        │   (Attestation & JWT)   │       │   (Log Schema & Sink)   │
        └────────────┬────────────┘       └────────────┬────────────┘
                     │                                 │
                     │ Produce                         │ Produce
                     │ (device-metadata)               │ (mobile-logs)
                     ▼                                 ▼
        ┌───────────────────────────────────────────────────────────┐
        │                 Apache Kafka (KRaft Bus)                  │
        └─────────────────────────────┬─────────────────────────────┘
                                      │ Consume
                                      ▼
                          ┌───────────────────────┐
                          │      Fluent Bit       │
                          │    (Log Forwarder)    │
                          └───────────┬───────────┘
                                      │ Bulk POST
                                      ▼
                          ┌───────────────────────┐
                          │      OpenSearch       │
                          │  & Dashboards (:5601) │
                          └───────────────────────┘
```

### Control & Configuration Plane

The control plane coordinates service discovery and hot-reloadable channel routing:
1. **HashiCorp Consul** acts as the service registry and configuration KV store (`gateway/config/global` and `gateway/config/channels/{id}`).
2. **`logger_admin` (Axum + Web UI)** provides an intuitive dashboard on `:8082` (secured by `ADMIN_TOKEN`) to manage channels and attestation policies.
3. **Hot-Reloading:** `auth_worker` runs a background task watching Consul KV using HTTP blocking queries, propagating attestation policy changes in real time without downtime. `logger_gateway` watches Consul service health to dynamically discover replicas of both `auth_worker` and `validation_worker`.

```
  ┌────────────────────────┐         ┌────────────────────────┐
  │      logger_admin      │ ──────▶ │    HashiCorp Consul    │
  │   (Web UI & API :8082) │  Writes │       (KV & DNS)       │
  └────────────────────────┘         └───────────┬────────────┘
                                                 │
                                                 ├── Dynamic Discovery ──▶ logger_gateway
                                                 └── Channel Config KV ──▶ auth_worker
```

---

### Services at a Glance

| Service                 | Source / Image                                   | Exposed Port                            | Role                                                          |
|-------------------------|--------------------------------------------------|-----------------------------------------|---------------------------------------------------------------|
| `logger_gateway`        | `./logger_gateway` (Pingora)                     | `8080` (API), `9090` (internal metrics) | Client-facing proxy, JWT validation, Consul discovery         |
| `auth_worker`           | `./auth_worker` (Axum)                           | `8001` (API native) / `8000` (Docker)   | Attestation verification, ES256 JWT minting, startup metadata |
| `validation_worker`     | `./validation_worker` (Axum)                     | `8000` (API), `9091` (metrics)          | Stateless log schema validation, Kafka producer (`mobile-logs`)|
| `logger_admin`          | `./logger_admin` (Axum + Web UI)                 | `8082`                                  | Channel configuration UI & REST API for Consul KV             |
| `consul`                | `hashicorp/consul:1.15`                          | `8500` (UI & API)                       | Dynamic service discovery and hot-reloadable configuration KV |
| `broker`                | `apache/kafka:4.3.0`                             | `9092` (host), `9094` (internal)        | KRaft distributed message bus                                 |
| `fluent-bit`            | `fluent/fluent-bit:latest`                       | `2020` (metrics)                        | Kafka consumer → OpenSearch bulk indexing forwarder           |
| `opensearch`            | `opensearchproject/opensearch:latest`            | `9200`                                  | Log document indexing and search engine                       |
| `opensearch-dashboards` | `opensearchproject/opensearch-dashboards:latest` | `5601`                                  | Kibana-compatible visualization dashboard                     |
| `kafka-ui`              | `provectuslabs/kafka-ui:latest`                  | `8081`                                  | Web browser for Kafka topics and consumer groups              |
| `prometheus`            | `prom/prometheus:v2.52.0`                        | `9090`                                  | Metrics scraper & time-series database                        |
| `grafana`               | `grafana/grafana:11.0.0`                         | `3000`                                  | Pre-provisioned system performance dashboards                 |
| `kafka-exporter`        | `danielqsj/kafka-exporter`                       | internal                                | Scrapes broker lag & offsets into Prometheus                  |
| `opensearch-exporter`   | `prometheuscommunity/elasticsearch-exporter`     | internal                                | Scrapes OpenSearch cluster health into Prometheus             |

---

## Project Structure

```
remote_logger/
├── logger_gateway/           # Pingora reverse proxy (Rust binary crate)
│   ├── src/
│   │   ├── auth/             # ES256 JWT validation
│   │   ├── config/           # Consul KV config loader & blocking watcher
│   │   ├── discovery.rs      # Consul health service discovery & load balancers
│   │   ├── metrics/          # Prometheus metrics exporter
│   │   ├── proxy/            # ProxyHttp implementation (header enrichment, dynamic routing)
│   │   └── main.rs
│   └── config/
│
├── auth_worker/              # Dedicated authentication service (Rust lib + bin crate)
│   ├── src/
│   │   ├── auth/             # Play Integrity & App Attest verifiers, ES256 minting
│   │   ├── config/           # Consul KV channel subscriber
│   │   ├── domain/           # Models (AuthExchangeRequest, DeviceMetadata, SystemStartupEvent)
│   │   ├── handler/          # Attestation & JWT exchange (/auth/exchange)
│   │   ├── kafka/            # Rdkafka sink for device-metadata topic
│   │   └── main.rs
│   └── Dockerfile
│
├── validation_worker/        # Stateless log ingestion worker (Rust lib + bin crate)
│   ├── src/
│   │   ├── domain/           # Shared models (ServerLog, KafkaPayload)
│   │   ├── handler/          # Ingestion (/v1/logs/:channel, /batch, /ingest)
│   │   ├── kafka/            # Rdkafka sink with acknowledgment guarantees
│   │   └── main.rs
│   ├── Dockerfile
│   └── tests/
│       └── integration_test.rs # Ephemeral testcontainers Kafka integration suite
│
├── logger_admin/             # Configuration administration UI & API (Rust crate)
│   ├── src/                  # Axum REST endpoints for Consul KV management
│   └── static/               # HTML5 / CSS / Vanilla JS Single-Page Application
│
├── mobile/                   # Client SDK libraries
│   ├── remote_logging/       # Pure Dart logging engine with batching & truncation
│   └── flutter_remote_logging/ # Flutter extension with device info & SQLite offline cache
│
├── docker/                   # Local development container stack
│   ├── docker-compose.yaml   # Base compose definition
│   ├── docker-compose.dev.yaml # Contributor live-reload compose overrides
│   ├── keys/                 # Development ECDSA P-256 key pair (public & private PEM)
│   ├── monitoring/           # Prometheus scrape configuration & Grafana dashboards
│   └── .env.template         # Environment variable template
│
├── kubernetes/               # Production Kubernetes manifests (Kustomize)
│   ├── broker/               # Kafka KRaft StatefulSet
│   ├── consul/               # Consul agent Deployment & Service
│   ├── gateway/              # Logger gateway Deployment & Service
│   ├── validation-worker/    # Validation worker Deployment & Service
│   ├── opensearch/           # OpenSearch & Dashboards
│   ├── monitoring/           # Prometheus, Grafana, exporters
│   └── DEPLOYMENT.md         # Step-by-step deployment guide
│
├── scripts/                  # Operational & testing scripts
│   ├── send_logs.sh          # Interactive CLI smoke test tool
│   └── verify_metrics.sh     # Automated Prometheus metrics health checker
│
└── load_tests/               # Performance & stress testing suite
    ├── load_test.sh          # Dependency-free bash/curl parallel load tool
    ├── stress_test.js        # k6 step-by-step VU ramp-up scenario
    └── run_k6.sh             # Dockerized k6 runner script
```
