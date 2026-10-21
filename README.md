# Remote Logger

A high-throughput, multi-tenant mobile log ingestion and search pipeline built with **Rust**, **Apache Kafka**, and **OpenSearch**.

Remote Logger enables mobile applications to stream telemetry and diagnostic logs securely at scale. Built with Cloudflare's **Pingora** reverse proxy and **Axum** microservices, the system enforces hardware key attestation (Android Keystore Key Attestation & Apple App Attest / Secure Enclave), mints cryptographic ES256 session tokens, buffers events through Kafka, and indexes them into OpenSearch for real-time query and dashboard visualization.

Includes native client SDKs for **Dart** and **Flutter** with built-in batching, truncation, and offline persistence.

---

## 📑 Table of Contents

- [Key Features](#-key-features)
- [Roadmap / TODO](#-roadmap--todo)
- [High-Level Architecture](#high-level-architecture)
- [Prerequisites](#prerequisites)
- [Local Development Setup](#local-development-setup)
- [Configuration & Channel Management](#configuration--channel-management)
- [Authentication & Cryptography](#authentication--cryptography)
- [Log Ingestion Format](#log-ingestion-format)
- [Running Tests](#running-tests)
- [Environment Variables](#environment-variables)
- [Security](#security)
- [License](#license)

---

## 🚀 Key Features

- 🛡️ **Offline Hardware Key Attestation**: Verify authentic devices and application builds using Android Keystore Key Attestation (StrongBox / TEE) and Apple App Attest / Secure Enclave without external cloud API dependencies.
- ⚡  **High-Performance Ingestion**: Powered by Pingora and Axum for zero-copy streaming, sub-millisecond overhead, and memory efficiency.
- 🔒 **Stateless ES256 Authentication**: Two-tier 90-day hardware key enrollment + per-session ECDSA P-256 proofs of possession with channel binding and identity enrichment.
- 📡 **Zero-Data-Loss Reliability**: Kafka producer pipeline waits for broker ACKs prior to client response.
- 🔄 **Dynamic Hot-Reloading**: Channels, attestation rules, and routing live in HashiCorp Consul KV with zero-downtime subscriber updates.
- 📱 **Ready-to-Use Mobile SDKs**: First-class Dart core and Flutter plugins with local SQLite offline caching and queue recovery.
- 📊 **Turnkey Observability**: Out-of-the-box Prometheus metrics, Grafana dashboards, and OpenSearch log search UIs.

---

## 📌 Roadmap / TODO

- [x] **Split Auth Service from Validation Worker**: Decouple hardware attestation and session JWT issuance (`/auth/exchange`) into a dedicated authentication microservice (`auth_worker`), keeping `validation_worker` focused strictly on log validation and Kafka ingestion.
- [x] **Hardware Attestation Production Readiness & Documentation**: End-to-end offline cryptographic verification for Android Keystore Key Attestation (`strongbox` / `tee`) and Apple App Attest / Secure Enclave (`secure_enclave`) with full Logger Admin UI support and setup guides.

---

## Prerequisites

| Tool           | Minimum Version          | Notes                                               |
|----------------|--------------------------|-----------------------------------------------------|
| Rust + Cargo   | 1.82+                    | `rustup update stable`                              |
| Docker Engine  | 24+                      | Required for local container stack                  |
| Docker Compose | v2 (`docker compose`)    | Bundled with Docker Desktop                         |
| CMake          | any                      | Required if compiling `rdkafka` locally from source |
| Dart / Flutter | Dart 3.10+ / Flutter 3.x | Required for mobile package development             |

---

## High-Level Architecture

The platform decouples edge ingestion from compute-intensive indexing using an event-driven streaming pipeline:

```
Mobile Clients ──▶ logger_gateway (:8080) ──┬──▶ auth_worker (:8001 / :8000) (Attestation & JWT)
                                            └──▶ validation_worker (:8000) ──▶ Apache Kafka ──▶ Fluent Bit ──▶ OpenSearch (:9200)
```

- **Data Plane:** Mobile clients authenticate and ingest logs via Cloudflare Pingora (`logger_gateway`). `/auth/*` requests (`/auth/challenge`, `/auth/enroll`, `/auth/exchange`) are routed to `auth_worker` for hardware attestation, 90-day key enrollment, and ES256 session token issuance. Ingestion requests are validated for bearer tokens and routed to Axum (`validation_worker`) instances, which validate payload schemas and publish directly to Apache Kafka. Fluent Bit consumes Kafka streams and bulk-indexes into OpenSearch.
- **Control Plane:** HashiCorp Consul provides dynamic worker discovery and stores channel definitions in KV, managed via the `logger_admin` dashboard (:8082).

📖 **For detailed architecture diagrams, service roles, and project layout, see [ARCHITECTURE.md](ARCHITECTURE.md).**
🛡️ **For hardware attestation setup, channel modes, and activation steps, see [docs/attestation.md](docs/attestation.md).**

---

## Local Development Setup

### 1. Clone Repository and Prepare Environment

```bash
git clone https://github.com/femrek/remote_logger.git
cd remote_logger

# Copy the environment template
cp docker/.env.template docker/.env

# Generate a strong Admin Token for logger_admin UI/API
# and set it in docker/.env:
# GATEWAY_ADMIN_TOKEN=<your-token>
vim docker/.env
```

The repository provides pre-generated development ECDSA P-256 keys inside `docker/keys/` (`private.pem` and
`public.pem`). For production, generate a fresh pair:

```bash
openssl ecparam -name prime256v1 -genkey -noout -out docker/keys/private.pem
openssl ec -in docker/keys/private.pem -pubout -out docker/keys/public.pem
```

### 2. Start the Stack

#### Option A: Pull and Run Stable Pre-built Images

```bash
cd docker
docker compose up -d
```

#### Option B: Build Local Code with Live-Reloading

```bash
cd docker
docker compose -f docker-compose.yaml -f docker-compose-dev.yaml up -d --build
```

Wait for services to reach healthy status:

```bash
docker compose ps
```

---

### 3. Send a Test Log

**Single Request via CLI Helper:**

```bash
bash scripts/send_logs.sh
```

This script exchanges an attestation payload for an ES256 session token and POSTs a log event to the default
`example-app-dev` channel.

**Bulk Request:**

```bash
bash scripts/send_logs.sh --bulk 10
```

**Run Load Tests:**

```bash
# Lightweight curl-based concurrency test (200 requests, 10 workers)
bash load_tests/load_test.sh

# Advanced step-by-step k6 stress test (runs in Docker)
bash load_tests/run_k6.sh -q
```

See [`load_tests/README.md`](load_tests/README.md) for full configuration options.

---

### 4. Inspect Results

| Service                   | URL                                         | Credentials / Access                               |
|---------------------------|---------------------------------------------|----------------------------------------------------|
| **Grafana Dashboards**    | http://localhost:3000                       | `admin` / (set `GRAFANA_ADMIN_PASSWORD` in `.env`) |
| **Logger Admin UI**       | http://localhost:8082                       | Enter `GATEWAY_ADMIN_TOKEN` when prompted          |
| **Consul UI**             | http://localhost:8500                       | Open catalog to view healthy services & KV keys    |
| **OpenSearch Dashboards** | http://localhost:5601                       | Browse indexed log entries                         |
| **Kafka UI**              | http://localhost:8081                       | Inspect `mobile-logs` and `device-metadata` topics |
| **OpenSearch REST API**   | http://localhost:9200/mobile-logs-*/_search | Direct JSON query API                              |
| **Prometheus Targets**    | http://localhost:9090/targets               | Check scrape health of all components              |

To run an automated health check against all metrics endpoints:

```bash
bash scripts/verify_metrics.sh
```

### 5. Tear Down

```bash
cd docker
docker compose down -v   # -v cleans named volumes (OpenSearch index, Kafka data)
```

---

## Configuration & Channel Management

Channel routing, attestation enforcement, and gateway parameters are dynamic and stored in **Consul KV**. Both
`logger_gateway` (global config) and `auth_worker` (channel configs) subscribe to changes via Consul HTTP blocking
queries, reloading configurations with zero downtime.

### Managing via Logger Admin UI

Open `http://localhost:8082` in your browser and enter your `GATEWAY_ADMIN_TOKEN`. The UI allows you to:

- Configure global settings (e.g. `max_body_bytes`, `jwt.issuer`).
- Create and edit application channels:
    - `id`: Routing channel slug (e.g. `my-app-prod`).
    - `app_id`: Application bundle/package ID (e.g. `com.company.app`).
    - `android_attestation_mode`: Android attestation enforcement (`off`, `monitor`, or `enforce`).
    - `ios_attestation_mode`: iOS attestation enforcement (`off`, `monitor`, or `enforce`).
    - `android_signing_cert_sha256` *(optional)*: Allowed Android APK signing certificate SHA-256 digests (skipped when empty so debug/release builds work out-of-the-box).
    - `apple_team_id` *(optional)*: Apple Developer Team ID for full iOS App Attest CBOR verification (falls back to Secure Enclave signature verification when omitted).
    - `apple_app_attest_env` *(optional)*: Apple App Attest environment (`development` or `production`).

### Consul KV Key Structure

- `gateway/config/global`: JSON object containing global settings:
  ```json
  {
    "jwt": { "issuer": "logger-gateway", "ttl_seconds": 86400 },
    "max_body_bytes": 2097152
  }
  ```
- `gateway/config/channels/{id}`: JSON object for each registered channel:
  ```json
  {
    "id": "example-app-dev",
    "app_id": "dev.femrek.exampleapp.dev",
    "android_attestation_mode": "off",
    "ios_attestation_mode": "off"
  }
  ```

---

## Authentication & Cryptography

The pipeline enforces separation between network transport encryption and application session security:

1. **Two-Tier Hardware Attestation & Session JWTs (Mandatory):**
    - `GET /auth/challenge`: Issues a 5-minute stateless HMAC challenge.
    - `POST /auth/enroll`: Verifies Android Keystore Key Attestation or Apple App Attest against the challenge and mints a 90-day ES256 `enrollment` JWT binding the device's hardware P-256 public key (`pub_key`) and trust tier (`att`).
    - `POST /auth/exchange`: Verifies an ECDSA P-256 signature over a fresh challenge using the enrolled key (or direct attestation token) and mints an **ES256 (ECDSA P-256)** `access` JWT.
    - `auth_worker` signs tokens using `GATEWAY_PRIVATE_KEY_PATH` (`private.pem`).
    - `logger_gateway` verifies `Authorization: Bearer <token>` on ingest routes using `GATEWAY_PUBLIC_KEY_PATH` (`public.pem`).
    - The access token embeds `sub` (device ID), `app` (application bundle ID), `session_id`, and `att` (attestation tier: `strongbox`, `tee`, `secure_enclave`, or `none`).
    - See **[docs/attestation.md](docs/attestation.md)** for complete setup and activation instructions.
2. **Transport Layer Security (Optional):**
    - Pingora supports TLS termination directly at the gateway listener.
    - Configure `GATEWAY_TLS_CERT_PATH` (`server.crt`) and `GATEWAY_TLS_KEY_PATH` (`server.key`). If either variable is
      omitted, plain HTTP is used.

---

## Log Ingestion Format

### Endpoints

- **Single Record Ingestion:**
  `POST /v1/logs/{channel}`
- **Batch Record Ingestion:**
  `POST /v1/logs/{channel}/batch`

### Ingestion Headers

- `Authorization: Bearer <session_jwt>` (Required)
- `Content-Type: application/json`

### Record Schema (`ServerLog`)

```json
{
  "timestamp": "2026-10-02T12:00:00Z",
  "loggerName": "DatabaseService",
  "message": "Connection pool initialized.",
  "level": "INFO",
  "zone": "",
  "error": null,
  "stacktrace": null
}
```

> **Identity & Attestation Binding:** The gateway extracts verified `app_id` (`claims.app`), `device_id` (`claims.sub`), `session_id`, and `att` directly from the validated JWT claims and forwards them upstream in internal `x-app-id`, `x-device-id`, `x-session-id`, and `x-attestation` headers. The client body does not need to duplicate `deviceId`, and may optionally include a historical `sessionId` for offline-replayed logs.

For batch requests, wrap multiple records in a JSON array: `[ { ... }, { ... } ]`.

---

## Running Tests

### Tier 1 — Unit Tests *(In-process, ~0 ms)*

Tests validation rules, attestation flows, error handling, and routing using an in-memory `NoopSink`:

```bash
cd auth_worker && cargo test
cd ../validation_worker && cargo test --lib
cd ../logger_gateway && cargo test
```

### Tier 2 — Integration Tests *(Requires Docker, ~30 s)*

Spins up an ephemeral Kafka container via `testcontainers-rs`, runs the Axum application, produces records, and asserts
Kafka messages:

```bash
cd validation_worker
cargo test --test integration_test

# To skip:
SKIP_INTEGRATION=1 cargo test --test integration_test
```

### Tier 3 — End-to-End Tests *(Full stack, ~3 min)*

Spins up the complete pipeline (Gateway, Workers, Kafka, Fluent Bit, OpenSearch), exchanges tokens, posts unique log
markers, and queries OpenSearch REST:

```bash
cd e2e
cargo test --test e2e_test

# To skip:
SKIP_E2E=1 cargo test --test e2e_test
```

### Mobile SDK Tests

```bash
cd mobile/remote_logging && dart test
cd ../flutter_remote_logging && flutter test
```

---

## Environment Variables

| Variable                   | Default                        | Service             | Description                                          |
|----------------------------|--------------------------------|---------------------|------------------------------------------------------|
| `GATEWAY_PRIVATE_KEY_PATH` | `/etc/logger/keys/private.pem` | `auth_worker`       | Path to ECDSA P-256 private key for JWT minting      |
| `GATEWAY_PUBLIC_KEY_PATH`  | `/etc/logger/keys/public.pem`  | `logger_gateway`    | Path to ECDSA P-256 public key for JWT verification  |
| `GATEWAY_BIND_ADDR`        | `0.0.0.0:8080`                 | `logger_gateway`    | Gateway HTTP listener address                        |
| `GATEWAY_ADMIN_TOKEN`      | —                              | `logger_admin`      | Bearer secret required for Admin UI & API operations |
| `CONSUL_HTTP_ADDR`         | `http://consul:8500`           | all services        | Consul agent HTTP address for discovery and KV sync  |
| `KAFKA_BROKERS`            | `broker:9094`                  | workers             | Comma-separated Kafka bootstrap servers              |
| `PORT`                     | `8000` (`8001` auth native)    | workers             | Internal API port                                    |
| `METRICS_PORT`             | `9091` (`9092` auth native)    | workers             | Worker Prometheus metrics port                       |
| `GATEWAY_TLS_CERT_PATH`    | —                              | `logger_gateway`    | Optional SSL/TLS certificate path                    |
| `GATEWAY_TLS_KEY_PATH`     | —                              | `logger_gateway`    | Optional SSL/TLS private key path                    |
| `RUST_LOG`                 | `info`                         | all services        | Tracing log filter directives                        |
| `GRAFANA_ADMIN_PASSWORD`   | `admin`                        | `grafana`           | Grafana administrator credentials                    |

---

## Security

If you discover a security vulnerability or sensitive flaw related to authentication, hardware attestation, or network
proxying, **please do not open a public issue**.

Instead, please report it privately via GitHub Security Advisories or email the maintainer directly at
`faruk.eksioglu@hotmail.com`.

---

## License

This project is licensed under the [MIT License](LICENSE).
