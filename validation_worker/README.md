# Validation Worker (`validation_worker`)

The high-throughput, stateless log schema validation and Kafka producer microservice in the Remote Logger pipeline, built on [Axum](https://github.com/tokio-rs/axum) and [rdkafka](https://github.com/fede1024/rust-rdkafka).

> **Note:** Hardware attestation, challenge issuance, enrollment, and ES256 JWT minting live in the dedicated [`auth_worker`](../auth_worker/README.md) service.

---

## Responsibilities

1. **Log Ingestion & Schema Validation**:
   - Endpoints: `POST /v1/logs/:channel`, `POST /v1/logs/:channel/batch`, and `POST /ingest`.
   - Validates incoming single or batched `ServerLog` JSON payloads.
   - Extracts verified client identity and attestation tier from internal headers injected by `logger_gateway` (`x-app-id`, `x-device-id`, `x-session-id`, `x-attestation`).
   - Preserves client-supplied `sessionId` on historical/offline replayed logs while backfilling missing `sessionId` and `deviceId` from gateway headers.
   - Flattens records into `KafkaPayload` enriched with OpenSearch index prefixes (`mobile-logs-{app_id}`).
2. **Kafka Delivery with Acknowledgment Guarantees**:
   - Produces validated log messages to Apache Kafka (`mobile-logs` topic) using `rdkafka`.
   - Awaits broker acknowledgment before returning `200 OK` (zero data loss guarantee).
3. **Consul Service Registration**:
   - Registers itself as `validation_worker` in HashiCorp Consul with a periodic HTTP `/health` check for dynamic load balancing by `logger_gateway`.
4. **Observability**:
   - Exports Prometheus metrics on `:9091` (`worker_requests_total`, `worker_request_duration_seconds`, `worker_kafka_produce_total`).

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8000` | HTTP listener port |
| `METRICS_PORT` | `9091` | Prometheus `/metrics` port |
| `KAFKA_BROKERS` | `localhost:9092` | Kafka bootstrap brokers |
| `KAFKA_TOPIC` | `mobile-logs` | Ingest log topic name |
| `CONSUL_HTTP_ADDR` | `http://localhost:8500` | Consul agent endpoint for service registration |
| `ADVERTISED_IP` | `auto` | IP address advertised to Consul service discovery |
| `NODE_ID` | `auto` | Unique service instance identifier for Consul registration |
| `RUST_LOG` | `info` | Tracing filter directive |

---

## Testing

```bash
# Unit tests (Tier 1, in-process NoopSink)
cargo test --lib

# Integration tests (Tier 2, ephemeral Kafka testcontainers)
cargo test --test integration_test
```
