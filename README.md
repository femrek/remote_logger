# Remote Logger Backend

A high-throughput, multi-tenant mobile log ingestion pipeline built in Rust.
The system accepts structured log events from mobile clients, validates and
routes them through a Kafka broker, and indexes them into OpenSearch for
search and analysis.

---

## Architecture

```
Mobile Client (Dart)
        │  HTTP POST /v1/logs/{channel}
        │  Bearer <session JWT>
        ▼
┌──────────────────────────────────────────────────────────────┐
│  logger_gateway  (Pingora reverse proxy)  :8080              │
│  • Terminates client TLS & manages load-balancing            │
│  • Per-channel routing (gateway.yaml, hot-reloadable)        │
│  • Verifies ingest JWT signature & strips auth headers       │
│  • Injects x-tenant-id / x-app-id upstream identity headers  │
└────────────────────────┬─────────────────────────────────────┘
                         │  HTTP POST /v1/logs/{channel}
                         │  x-tenant-id, x-app-id headers
                         ▼
┌──────────────────────────────────────────────────────────────┐
│  validation_worker  (Axum microservice)  :8000  × 2 replicas │
│  • Handles POST /auth/exchange (Attestation + JWT minting)   │
│  • Deep-validates inner ServerLog schema                     │
│  • Enriches with log_index_prefix (mobile-logs-{app_id})     │
│  • Awaits Kafka ACK before responding (zero data loss)       │
└────────────────────────┬─────────────────────────────────────┘
                         │  Kafka produce → "mobile-logs" topic
                         ▼
              Apache Kafka  (KRaft mode)  :9092 / :9094
                         │
                         │  Kafka consume
                         ▼
              Fluent Bit  (Flush every 2 s)
                         │  Bulk POST
                         ▼
              OpenSearch  :9200
              OpenSearch Dashboards  :5601
```

### Services at a glance

| Service | Image / Source | Exposed Port | Role |
|---|---|---|---|
| `logger_gateway` | `./logger_gateway` (Rust/Pingora) | `8080`, `9090` (metrics) | Auth, routing, attestation |
| `validation_worker` | `./validation_worker` (Rust/Axum) | `8000` (API), `9091` (metrics) | Validation, Kafka publish |
| `broker` | `apache/kafka:4.3.0` | internal | Message broker |
| `fluent-bit` | `fluent/fluent-bit:latest` | `2020` (metrics) | Kafka → OpenSearch forwarder |
| `opensearch` | `opensearchproject/opensearch:latest` | `9200` | Log index & search |
| `opensearch-dashboards` | `opensearchproject/opensearch-dashboards:latest` | `5601` | Kibana-compatible log explorer UI |
| `kafka-ui` | `provectuslabs/kafka-ui:latest` | `127.0.0.1:1234` | Kafka topic browser |
| `prometheus` | `prom/prometheus:v2.52.0` | `127.0.0.1:9091` | Metrics scraper & TSDB |
| `grafana` | `grafana/grafana:11.0.0` | `3000` | Dashboards & visualisation |
| `kafka-exporter` | `danielqsj/kafka-exporter` | internal | Kafka metrics → Prometheus |
| `opensearch-exporter` | `prometheuscommunity/elasticsearch-exporter` | internal | OpenSearch metrics → Prometheus |

---

## Project Structure

```
remote_logger_backend/
├── logger_gateway/          # Pingora reverse proxy (binary crate)
│   ├── src/
│   │   ├── auth/            # JWT issuance + attestation verifiers
│   │   ├── config/          # YAML config loader + hot-reload watcher
│   │   ├── discovery.rs     # File-based dynamic worker discovery
│   │   ├── envelope/        # IngestEnvelope construction
│   │   ├── metrics/         # Prometheus endpoint (:9090)
│   │   ├── proxy/           # ProxyHttp implementation (routing logic)
│   │   └── main.rs
│   └── config/
│       └── gateway.yaml     # Channel routing table (hot-reloadable)
│
├── validation_worker/       # Axum microservice (lib + bin crate)
│   ├── src/
│   │   ├── metrics/         # Prometheus endpoint (:9091)
│   │   ├── handler/         # ingest + auth handlers (instrumented)
│   │   ├── kafka/           # RdkafkaSink (instrumented)
│   │   ├── lib.rs           # KafkaSink trait, handlers, domain types, unit tests
│   │   └── main.rs          # Binary entry-point (thin wrapper)
│   └── tests/
│       └── integration_test.rs   # Tier-2 tests (testcontainers Kafka)
│
├── e2e/                     # Standalone E2E test crate
│   ├── tests/
│   │   └── e2e_test.rs      # Tier-3 full-stack test (docker compose)
│   └── .env.test            # E2E environment variables
│
├── docker/
│   ├── docker-compose.yaml   # Full local stack definition (includes monitoring)
│   ├── fluent-bit.conf       # Kafka input → OpenSearch output config
│   ├── workers.txt           # Validation worker addresses (discovery file)
│   ├── monitoring/
│   │   ├── prometheus.yml    # Prometheus scrape config (5 targets)
│   │   └── grafana/
│   │       ├── provisioning/ # Auto-provisioned datasource + dashboard provider
│   │       └── dashboards/   # Pre-built logger-backend.json dashboard
│   ├── .env                  # Active environment (gitignored)
│   └── .env.template         # Copy → .env and fill in secrets
│
├── kubernetes/
│   ├── monitoring/           # Prometheus + Grafana + exporters manifests
│   │   ├── prometheus/       # Deployment, ConfigMap, RBAC
│   │   ├── grafana/          # Deployment, Secret, ConfigMaps
│   │   ├── kafka-exporter/   # Deployment + Service
│   │   └── opensearch-exporter/ # Deployment + Service
│   └── DEPLOYMENT.md
│
├── scripts/
│   ├── send_logs.sh          # Manual smoke-test helper
│   └── verify_metrics.sh     # Smoke-tests all Prometheus /metrics endpoints
│
├── mobile/                  # Dart client library
└── log_data_specification_report.md   # Schema & discrepancy analysis
```

---

## Prerequisites

| Tool | Minimum version | Notes |
|---|---|---|
| Rust + Cargo | 1.82 | `rustup update stable` |
| Docker Engine | 24 | Docker Desktop works too |
| Docker Compose | v2 (`docker compose`) | Bundled with Docker Desktop |
| cmake | any | Required to compile `rdkafka` (`cmake-build` feature) |

---

## Deployment

| Method | Guide |
|---|---|
| **Docker Compose** (local / dev) | See [Local Development Setup](#local-development-setup) below |
| **Kubernetes** (staging / production) | [`kubernetes/DEPLOYMENT.md`](kubernetes/DEPLOYMENT.md) |

---

## Local Development Setup

### 1. Clone and prepare environment

```bash
git clone <repo-url>
cd remote_logger_backend

# Copy the env template
cp docker/.env.template docker/.env

# Edit docker/.env — at minimum set GATEWAY_JWT_SECRET
# (any non-empty string works for local dev)
vim docker/.env
```

### 2. Start the full stack

Depending on your audience, choose one of the options below:

#### Option A: Run stable public images (For Users)
To run the pre-built latest stable images pulled from the registry:
```bash
cd docker
docker compose up -d
```

#### Option B: Build and run local code (For Contributors / Developers)
To run and develop using local code changes with live-reloading (via overrides and volumes):
```bash
cd docker
docker compose -f docker-compose.yaml -f docker-compose.dev.yaml up -d --build
```
Or use the native `watch` capability:
```bash
docker compose -f docker-compose.yaml -f docker-compose.dev.yaml watch
```

`docker compose up --wait` (or adding `--wait` to the commands above) can be used to block until all service healthchecks pass.

### 3. Send a test log

**Single request:**
```bash
bash scripts/send_logs.sh
```

**Load test** (generates real traffic visible in Grafana):
```bash
# 200 requests, 10 concurrent — default
bash scripts/load_test.sh

# Custom: 1000 requests, 20 concurrent
bash scripts/load_test.sh -n 1000 -c 20
```

The script exchanges an attestation token for a JWT, warms up with one request, then fires
the full load concurrently. Results show success count, duration, and ~throughput.

### 4. Inspect results

| UI | URL | Credentials |
|---|---|---|
| **Grafana Dashboards** | http://localhost:3000 | `admin` / `admin` (set `GRAFANA_ADMIN_PASSWORD` in `.env`) |
| **Prometheus** | http://localhost:9091 | — |
| OpenSearch Dashboards | http://localhost:5601 | — |
| Kafka UI | http://localhost:1234 | — |
| OpenSearch REST | http://localhost:9200/mobile-logs-*/_search | — |
| Gateway metrics (raw) | http://localhost:9090/metrics | — |
| Worker metrics (raw) | http://localhost:9091/metrics | — |

> **Grafana** opens directly to the pre-built **Logger Backend — Full Stack** dashboard.
> Navigate to **Status → Targets** in Prometheus to confirm all 5 scrape targets are **UP**.

You can also run the automated smoke test to verify every metrics endpoint:

```bash
bash scripts/verify_metrics.sh
```

### 5. Tear down

```bash
cd docker
docker compose down -v   # -v removes the OpenSearch data volume
```

---

## Gateway Configuration

Channel routing is defined in [`logger_gateway/config/gateway.yaml`](logger_gateway/config/gateway.yaml).
The file is **hot-reloadable** — saving it triggers a zero-downtime config reload.

```yaml
channels:
  - id: "example-app-dev"
    path_prefix: "/v1/logs/example-app-dev"
    app_id: "dev.femrek.exampleapp.dev"
    validate_attestation: false   # skip attestation in dev
```

**JWT settings** (`gateway.jwt`):

| Key | Default | Description |
|---|---|---|
| `secret_env` | `GATEWAY_JWT_SECRET` | Env var holding the HMAC secret |
| `ttl_seconds` | `300` | Session token lifetime (5 minutes) |

---

## Log Payload Format

> See [`log_data_specification_report.md`](log_data_specification_report.md) for the full schema and a detailed discrepancy analysis.

The validation worker expects a raw **`ServerLog`** object payload matching the mobile client's log export format:

```json
{
  "timestamp":  "2026-06-23T15:00:00Z",
  "loggerName": "AppDatabase",
  "message":    "Connection established.",
  "level":      "INFO",
  "zone":       "",
  "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
  "sessionId":  "session-123"
}
```

The gateway injects `x-tenant-id` and `x-app-id` headers derived from the requested channel configuration. The worker validates the raw `ServerLog` strictly against its schema and flattens it with the injected identities before forwarding it to Kafka.

---

## Running Tests

The test suite is split into three tiers with increasing infrastructure requirements.

### Tier 1 — Unit Tests *(no Docker, ~0 ms)*

Tests the `validation_worker` handler logic entirely in-process using a
`NoopSink` (no real Kafka connection). Covers valid payloads, all 400 error
paths, and the routing quirks documented in the spec report.

```bash
cd validation_worker
cargo test --lib
```

Expected output:
```
running 9 tests
test tests::test_valid_envelope_returns_200 ... ok
test tests::test_ingest_alias_route_returns_200 ... ok
test tests::test_missing_device_uuid_returns_400 ... ok
test tests::test_invalid_json_returns_400 ... ok
test tests::test_missing_app_id_header_returns_401 ... ok
test tests::test_session_id_is_optional_and_omission_returns_200 ... ok
test tests::test_valid_server_log_with_headers_returns_200 ... ok
test tests::test_optional_error_and_stacktrace_fields_accepted ... ok
test tests::test_path_with_log_suffix_returns_404 ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured
```

To skip: not applicable — these tests have no external dependencies.

---

### Tier 2 — Integration Tests *(Docker required, ~30 s)*

Spins up an **ephemeral Kafka container** via
[testcontainers](https://github.com/testcontainers/testcontainers-rs), then:

1. Builds the real Axum app connected to that container.
2. Sends HTTP requests via `reqwest`.
3. Consumes from the `mobile-logs` topic with an rdkafka `StreamConsumer`.
4. Asserts the **flattened** `KafkaPayload` structure field-by-field (no nested
   `payload` key — spec report §4).

```bash
cd validation_worker
cargo test --test integration_test
```

To skip without removing the tests:

```bash
SKIP_INTEGRATION=1 cargo test --test integration_test
```

> **Note:** The Confluent Kafka image used by testcontainers is downloaded on
> first run (~700 MB) and cached by Docker for subsequent runs.

---

### Tier 3 — E2E Tests *(Docker + full stack, ~3 min)*

Exercises the **entire pipeline** from HTTP POST through to an OpenSearch index
assertion. The test:

1. Reads `e2e/.env.test` for configuration.
2. Runs `docker compose` with both `docker-compose.yaml` and `docker-compose.dev.yaml` (if it exists) using `up -d --wait` to start the
   full stack (gateway, workers, Kafka, Fluent Bit, OpenSearch).
3. Calls `POST /auth/exchange` to obtain a real session JWT.
4. Posts a log with a unique timestamp-based message marker to
   `POST /v1/logs/example-app-dev`.
5. Waits 5 seconds for Fluent Bit's 2-second flush cycle to complete.
6. Queries `GET /mobile-logs-*/_search?q=message:<marker>` and asserts
   `hits.total.value > 0`.
7. Runs `docker compose down -v` on exit (even if the test fails).

**Setup — do this once:**

```bash
# Edit e2e/.env.test and set your JWT secret
# It must match the GATEWAY_JWT_SECRET in docker/.env
vim e2e/.env.test
```

```ini
# e2e/.env.test
SKIP_E2E=0
GATEWAY_JWT_SECRET=your-secret-here
```

**Run:**

```bash
cd e2e
cargo test --test e2e_test
```

To skip (e.g. in CI without Docker):

```bash
SKIP_E2E=1 cargo test --test e2e_test
# — or — set SKIP_E2E=1 in e2e/.env.test
```

> **Important:** The E2E test will call `docker compose down -v` on completion,
> which removes the `opensearch-data` named volume. Any data indexed during a
> previous manual run will be lost.

---

### Quick reference — all tiers

```bash
# Tier 1: unit (no Docker needed)
cd validation_worker && cargo test --lib

# Tier 2: integration (Docker needed)
cd validation_worker && cargo test --test integration_test
SKIP_INTEGRATION=1 cargo test --test integration_test   # skip

# Tier 3: E2E (Docker + full stack)
cd e2e && cargo test --test e2e_test
SKIP_E2E=1 cargo test --test e2e_test                   # skip
```

---

## Environment Variables

| Variable | Default | Service | Description |
|---|---|---|---|
| `GATEWAY_JWT_SECRET` | — | `logger_gateway`, `validation_worker` | HMAC-SHA256 secret for JWT signing/verification. **Required.** |
| `RUST_LOG` | `info` | both | Tracing filter (e.g. `info,logger_gateway=debug`) |
| `WORKERS_FILE` | `config/workers_list.json` | `logger_gateway` | Path to the dynamic worker address file |
| `KAFKA_BROKERS` | `localhost:9092` | `validation_worker` | Comma-separated Kafka bootstrap servers |
| `PORT` | `3000` | `validation_worker` | HTTP API listener port |
| `METRICS_PORT` | `9091` | `validation_worker` | Prometheus `/metrics` listener port |
| `GRAFANA_ADMIN_PASSWORD` | `admin` | `grafana` | Grafana admin password (change before any non-local deploy) |
| `SKIP_INTEGRATION` | unset | tests | Set to `1` to skip Tier-2 integration tests |
| `SKIP_E2E` | `0` | tests | Set to `1` to skip Tier-3 E2E tests |

---

## Known Limitations

These are documented in detail in [`log_data_specification_report.md`](log_data_specification_report.md).

1. **Route mismatch (§1):** The validation worker listens on `/v1/logs/:channel`
   (no trailing `/log`). Requests to `/v1/logs/:channel/log` return 404. Use the
   path without the suffix.

2. **Raw `ServerLog` Body (§2):** The worker expects the raw `ServerLog` directly, with identity managed via gateway headers. The payload must not be wrapped.

3. **`sessionId` missing from mobile client (§3):** The Dart client never
   serialises `sessionId`. It is stored as `null` in OpenSearch.

4. **Fluent Bit `nest/lift` filter is a no-op (§4):** The worker flattens
   `ServerLog` into the Kafka message root; there is no `payload` key to lift.
