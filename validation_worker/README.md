# Validation Worker (`validation_worker`)

The core validation microservice in the Remote Logger pipeline, built on [Axum](https://github.com/tokio-rs/axum) and [rdkafka](https://github.com/fede1024/rust-rdkafka).

---

## Responsibilities

1. **Hardware Attestation & Token Minting (`POST /auth/exchange`)**:
   - Verifies mobile hardware attestations (Google Play Integrity on Android or Apple App Attest on iOS) according to channel policy.
   - Mints short-lived ES256 access JWTs and long-lived refresh JWTs signed with ECDSA P-256 (`GATEWAY_PRIVATE_KEY_PATH`).
   - Produces device metadata events (`SystemStartupEvent`) to the `device-metadata` Kafka topic.
2. **Log Ingestion & Schema Validation**:
   - Endpoints: `POST /v1/logs/:channel` and `POST /v1/logs/:channel/batch`.
   - Validates incoming `ServerLog` JSON objects.
   - Extracts identity from headers injected by `logger_gateway` (`x-app-id`, `x-device-uuid`, `x-session-id`).
   - Flattens records into `KafkaPayload` enriched with OpenSearch index prefixes (`mobile-logs-{app_id}`).
3. **Kafka Delivery with Acknowledgment Guarantees**:
   - Produces messages to Apache Kafka using `rdkafka`.
   - Waits for broker acknowledgment before returning `200 OK` (zero data loss guarantee).
4. **Dynamic Channel Configuration**:
   - Subscribes to channel definitions in Consul KV (`gateway/config/channels/{id}`) and hot-reloads them dynamically.
5. **Observability**:
   - Prometheus metrics on `:9091` (`worker_requests_total`, `worker_kafka_produce_total`, `worker_jwt_operations_total`).

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8000` | HTTP listener port |
| `METRICS_PORT` | `9091` | Prometheus `/metrics` port |
| `KAFKA_BROKERS` | `localhost:9092` | Kafka bootstrap brokers |
| `KAFKA_TOPIC` | `mobile-logs` | Ingest log topic name |
| `KAFKA_METADATA_TOPIC` | `device-metadata` | Device metadata topic name |
| `GATEWAY_PRIVATE_KEY_PATH` | `./private_key.pem` | Path to ECDSA P-256 private key |
| `CONSUL_HTTP_ADDR` | `http://localhost:8500` | Consul agent endpoint |
| `ACCESS_JWT_TTL_SECONDS` | `900` | Access token lifespan (15 min) |
| `REFRESH_JWT_TTL_SECONDS` | `604800` | Refresh token lifespan (7 days) |

---

## Testing

```bash
# Unit tests (Tier 1, in-process NoopSink)
cargo test --lib

# Integration tests (Tier 2, ephemeral Kafka testcontainers)
cargo test --test integration_test
```
