# Remote Logger

A high-throughput, multi-tenant mobile log ingestion and search pipeline built with **Rust**, **Apache Kafka**, and **OpenSearch**.

Remote Logger enables mobile applications to log locally and stream telemetry to a centralized, high-throughput search database. Built with Cloudflare's **Pingora** reverse proxy and **Axum** microservices, the pipeline buffers events through **Apache Kafka**, indexes them into **OpenSearch** for real-time querying and dashboards, and provides session security with optional hardware key attestation.

Includes native client SDKs for [**Dart**](mobile/remote_logging) and [**Flutter**](mobile/flutter_remote_logging) with built-in batching, truncation, and offline persistence.

---

## 📑 Table of Contents

- [Key Features](#-key-features)
  - [Local Mode (Offline & Standalone)](#-local-mode-offline--standalone)
  - [Remote Mode (Client Sync & Centralized Pipeline)](#-remote-mode-client-sync--centralized-pipeline)
- [Roadmap / TODO](#-roadmap--todo)
- [Prerequisites](#prerequisites)
- [High-Level Architecture](#high-level-architecture)
- [Local Development Setup (Native)](#local-development-setup-native)
- [Deployment Options](#deployment-options)
- [Configuration & Channel Management](#configuration--channel-management)
- [Authentication & Cryptography](#authentication--cryptography)
- [Log Ingestion Format](#log-ingestion-format)
- [Running Tests](#running-tests)
- [Environment Variables](#environment-variables)
- [Security](#security)
- [License](#license)

---

## 🚀 Key Features

Remote Logger provides a unified logging experience across two operating modes:

### 📴 Local Mode (Offline & Standalone)

The client SDKs—available as both a pure [**Dart core package**](mobile/remote_logging) (`remote_logging`) and a [**Flutter plugin**](mobile/flutter_remote_logging) (`flutter_remote_logging`)—operate completely standalone without requiring any backend server or network connection:

- 📴 **Zero-Infrastructure Local Logging**: Works immediately out-of-the-box using standard, familiar `Logger` APIs from Dart [`package:logging`](https://pub.dev/packages/logging).
- 🖥️ **Console Formatting & Level Filtering**: Clean, readable console output with configurable log severity thresholds (`ALL`, `FINE`, `INFO`, `WARNING`, `SEVERE`), hierarchical zone tags, and support for custom `logPrinter` formatters.
- 🔔 **In-App `onLog` Callbacks**: Intercept log records in real time via an `onLog` callback—ideal for displaying logs in in-app debug panels, developer overlays, or diagnostic tools.
- 🔄 **Frictionless Transition to Remote**: Write standard `logger.info(...)` statements once. Transition from local console logging to remote synchronization simply by initializing the remote handler—no code refactoring needed.

---

### 🌐 Remote Mode (Client Sync & Centralized Pipeline)

When remote logging is enabled, the mobile SDK connects seamlessly to the centralized, high-throughput ingestion and indexing pipeline:

#### 📱 Mobile Plugin Features
- 💾 **SQLite Offline Persistence**: Integrated local queue powered by SQLite ([`flutter_remote_logging`](mobile/flutter_remote_logging)); logs survive network dead zones, app kills, and device reboots with automatic queue recovery.
- 📦 **Smart Batch Ingestion**: Queues records in memory and flushes them in batches over HTTP to minimize mobile radio wakeups, save bandwidth, and conserve battery life.
- 🔀 **Dual Logging Control**: Independently configure console output (`logPrintLevel`) and remote ingestion (`logSendLevel`), allowing verbose local debugging while streaming only high-priority events to the server.
- 🛡️ **Defensive Payload Truncation**: Configurable bounds on message lengths, error descriptions, and stack trace frames (`LogTruncationConfig`) applied at the source before queuing, ensuring the offline SQLite database and network payloads never hold oversized data.
- 🆔 **Automated Device & Session Metadata**: Automatically collects OS, OS version, device model, brand, manufacturer, app package identifier, persistent device UUIDs, and unique per-session UUIDs across Android, iOS, macOS, Windows, Linux, and Web.
- 🔌 **Pluggable & Extensible**: Pluggable network invokers (Dio or HTTP client), custom device metadata generators, and local cache strategies (see [example app](mobile/example)).
- 🛑 **Graceful Queue Flush & Teardown**: Explicit `flush()` and `stop({flush: true})` controls ensure all pending in-memory and SQLite batches are transmitted before application termination.

#### 🖥️ Centralized Remote System & OpenSearch Ingestion
- 🔍 **Centralized Indexed Database (OpenSearch)**: High-speed full-text search, multi-tenant index partitioning (`mobile-logs-{app_id}`), and instant document retrieval for millions of mobile telemetry events.
- 📊 **Turnkey Analytics & Dashboards**: Pre-configured OpenSearch Dashboards and Grafana visual analytics for real-time error rate tracking, crash investigations, latency percentiles, and device distribution metrics.
- ⚡ **High-Performance Ingestion Gateway**: Cloudflare Pingora reverse proxy providing zero-copy streaming, sub-millisecond overhead, and memory-efficient routing to upstream workers.
- 📡 **Zero-Data-Loss Kafka Streaming**: Microservice pipeline powered by Apache Kafka in KRaft mode; producers wait for broker ACKs before confirming ingestion requests.
- 🔄 **Dynamic Hot-Reloading via Consul**: Live routing tables, body size limits, and per-channel configurations synchronized dynamically via HashiCorp Consul KV with zero downtime.
- 🛡️ **Stateless Authentication & Optional Attestation**: Fast ES256 session tokens with lightweight validation at the gateway, backed by optional hardware key verification (Android Keystore / Apple App Attest) for untampered device assurance.

---

## 📌 Roadmap / TODO

- [ ] **iOS Attestation Real-Device Verification**: Validate Apple App Attest / Secure Enclave flows on physical iOS devices and production builds (*iOS attestation implementation has not been tested on real devices yet*).
- [ ] **Log Ingestion Idempotency & Deduplication**: Pass client-side log `id` through the pipeline as the OpenSearch document `_id` to eliminate duplicates during network retries and offline log replays.
- [ ] **Client Response Classification & Re-Auth**: Handle HTTP 401 in mobile SDKs with automatic token invalidation and re-auth, retry 429/5xx with exponential backoff, and drop unrecoverable 4xx errors.
- [ ] **Gateway Rate Limiting**: Implement per-session/device token-bucket rate limiting at `logger_gateway` to protect against token replay and log flooding.
- [ ] **Channel Binding in JWTs**: Add a `ch` claim during token exchange to enforce that access tokens can only ingest logs into their authorized channel.
- [ ] **JWT Signing Key Rotation & `kid` Support**: Support Key IDs (`kid`) and dual-key verification windows in `logger_gateway` for zero-downtime key rotation.
- [ ] **Android Keybox Revocation & Non-GMS Roots**: Periodically check Google's Keybox revocation list and add root certificate support for non-GMS Android devices (e.g. Huawei).

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

📖 **For detailed architecture diagrams, service roles, and project layout, see [docs/ARCHITECTURE.md](ARCHITECTURE.md).**
🛡️ **For hardware attestation setup, channel modes, and activation steps, see [docs/ATTESTATION.md](docs/ATTESTATION.md).**

---

## Local Development Setup (Native)

For rapid iteration and debugging, the Rust microservices can be compiled and run natively using Cargo. Backing infrastructure (Consul and Kafka) can be launched quickly via lightweight Docker containers.

### 1. Start Backing Infrastructure

The services require HashiCorp Consul (for discovery and dynamic channel config) and Apache Kafka (for message streaming). Start both backing services with exposed host ports:

```bash
# Start Consul (:8500) and Kafka (:9092)
docker compose -f docker/docker-compose.yaml -f docker/docker-compose-network.yaml up -d consul broker
```

Verify Consul is healthy:
```bash
curl -s http://localhost:8500/v1/status/leader
```

---

### 2. Prepare Cryptographic Keys

The repository includes pre-generated development keys in `docker/keys/` (`private.pem` and `public.pem`). For production, generate a fresh ECDSA P-256 key pair:

```bash
openssl ecparam -name prime256v1 -genkey -noout -out docker/keys/private.pem
openssl ec -in docker/keys/private.pem -pubout -out docker/keys/public.pem
```

---

### 3. Bootstrap Configuration with Logger Admin

Run `logger_admin` first. When pointed to a fresh Consul instance, it automatically seeds default global gateway settings and seed channels (`example-app-dev` and `example-app-prod`):

```bash
ADMIN_TOKEN=my-secret-admin-token \
CONSUL_HTTP_ADDR=http://localhost:8500 \
cargo run --manifest-path logger_admin/Cargo.toml
```

You can now open `http://localhost:8082` in your browser and enter `my-secret-admin-token` to inspect or create channels in the dashboard.

---

### 4. Run Workers and Gateway

Open separate terminal sessions for each microservice:

**Terminal 1 — Auth Worker** (attestation verification and ES256 JWT minting):
```bash
KAFKA_BROKERS=localhost:9092 \
GATEWAY_PRIVATE_KEY_PATH=docker/keys/private.pem \
CONSUL_HTTP_ADDR=http://localhost:8500 \
PORT=8001 \
METRICS_PORT=9092 \
cargo run --manifest-path auth_worker/Cargo.toml
```

**Terminal 2 — Validation Worker** (payload validation and Kafka producer):
```bash
KAFKA_BROKERS=localhost:9092 \
CONSUL_HTTP_ADDR=http://localhost:8500 \
PORT=8000 \
METRICS_PORT=9091 \
cargo run --manifest-path validation_worker/Cargo.toml
```

**Terminal 3 — Logger Gateway** (Pingora reverse proxy & JWT verification):
```bash
GATEWAY_PUBLIC_KEY_PATH=docker/keys/public.pem \
CONSUL_HTTP_ADDR=http://localhost:8500 \
GATEWAY_BIND_ADDR=0.0.0.0:8080 \
GATEWAY_METRICS_ADDR=0.0.0.0:9090 \
cargo run --manifest-path logger_gateway/Cargo.toml
```

---

### 5. Send Test Logs

With all services running, send a test log event through the gateway (`http://localhost:8080`):

**Single Request via CLI Helper:**
```bash
bash scripts/send_logs.sh
```

This exchanges an attestation challenge for an ES256 access token and submits a structured log to `example-app-dev`.

**Bulk Request:**
```bash
bash scripts/send_logs.sh --bulk 10
```

**Run Load Tests:**
```bash
# Lightweight curl-based concurrency test (200 requests, 10 workers)
bash load_tests/load_test.sh
```

---

### 6. Run Tests

```bash
# Rust service tests
cargo test --manifest-path logger_admin/Cargo.toml
cargo test --manifest-path auth_worker/Cargo.toml
cargo test --manifest-path validation_worker/Cargo.toml
cargo test --manifest-path logger_gateway/Cargo.toml

# Mobile SDK tests
cd mobile/remote_logging && dart test
cd ../flutter_remote_logging && flutter test
```

---

## Deployment Options

Remote Logger provides two turnkey deployment paths for non-development environments:

- 🐳 **Docker Compose Deployment**: Best suited for single-node virtual machines, dedicated servers, staging environments, and edge computing. Boots the complete pipeline (Pingora gateway, Axum workers, Kafka, Fluent Bit, OpenSearch, Consul, and Prometheus/Grafana) with persistent volumes and health checks.
  👉 **See the complete [Docker Compose Deployment Guide](docs/DEPLOYMENT_DOCKER.md).**

- ☸️ **Kubernetes Deployment**: Best suited for horizontally scalable cloud production clusters (EKS, GKE, AKS, or on-premise Kubernetes). Includes Kustomize manifests, multi-replica stateless validation workers, Consul-based service discovery, and zero-downtime rolling updates.
  👉 **See the complete [Kubernetes Deployment Guide](kubernetes/DEPLOYMENT.md).**

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

1. **Two-Tier Hardware Attestation & Session JWTs (Optional):**
    - `GET /auth/challenge`: Issues a 5-minute stateless HMAC challenge.
    - `POST /auth/enroll`: Verifies Android Keystore Key Attestation or Apple App Attest against the challenge and mints a 90-day ES256 `enrollment` JWT binding the device's hardware P-256 public key (`pub_key`) and trust tier (`att`).
    - `POST /auth/exchange`: Verifies an ECDSA P-256 signature over a fresh challenge using the enrolled key (or direct attestation token) and mints an **ES256 (ECDSA P-256)** `access` JWT.
    - `auth_worker` signs tokens using `GATEWAY_PRIVATE_KEY_PATH` (`private.pem`).
    - `logger_gateway` verifies `Authorization: Bearer <token>` on ingest routes using `GATEWAY_PUBLIC_KEY_PATH` (`public.pem`).
    - The access token embeds `sub` (device ID), `app` (application bundle ID), `session_id`, and `att` (attestation tier: `strongbox`, `tee`, `secure_enclave`, or `none`).
    - See **[docs/attestation.md](docs/ATTESTATION.md)** for complete setup and activation instructions.
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
