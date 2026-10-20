# Auth Worker (`auth_worker`)

The dedicated authentication and hardware attestation microservice in the Remote Logger pipeline, built on [Axum](https://github.com/tokio-rs/axum) and [rdkafka](https://github.com/fede1024/rust-rdkafka).

---

## Responsibilities

1. **Stateless Challenge Issuance (`GET /auth/challenge`)**:
   - Validates that the requested `X-Channel-Id` exists in the active channel configuration.
   - Issues a 5-minute (`300s`) HMAC-signed challenge (`<nonce>.<exp>.<sha256_sig>`) used to bind hardware key enrollment and per-session proofs of possession.
2. **Hardware Key Enrollment (`POST /auth/enroll`)**:
   - Verifies the stateless challenge and validates the device's hardware attestation statement (Android Keystore Key Attestation or Apple DeviceCheck App Attest) according to the channel's per-platform policy (`off`, `monitor`, or `enforce`).
   - Mints a 90-day (`7,776,000s`) ES256 enrollment JWT (`token_type: "enrollment"`) embedding `sub` (`device_id`), `app` (`app_id`), `plt` (`platform`), `att` (attestation trust tier), and `pub_key` (the client's hardware-backed P-256 public key).
3. **Session Token Exchange & Metadata Ingestion (`POST /auth/exchange`)**:
   - Supports two exchange flows:
     - **Enrollment Proof Flow**: Verifies the fresh challenge, decodes the 90-day `enrollment_token`, verifies that `app`, `sub`, and `plt` match the request, and verifies the client's ECDSA P-256 `challenge_signature` using the enrolled `pub_key`.
     - **Direct Attestation Flow**: Evaluates `attestation_token` directly against the channel's platform attestation mode (`off`, `monitor`, or `enforce`).
   - Mints an ES256 session access JWT (`token_type: "access"`) signed with `GATEWAY_PRIVATE_KEY_PATH` (`private.pem`).
   - Produces a `SystemStartupEvent` containing client `DeviceMetadata` to the `device-metadata` Kafka topic and awaits broker acknowledgment.
4. **Dynamic Channel Configuration & Service Registration**:
   - Bootstraps and watches `gateway/config/channels/` in HashiCorp Consul KV via HTTP blocking queries for zero-downtime policy reloads.
   - Registers itself as `auth_worker` in the Consul service catalog with an HTTP `/health` check for automatic discovery by `logger_gateway`.
5. **Observability**:
   - Exports Prometheus metrics on `:9092` (native default) or `:9091` (Docker/K8s) (`worker_requests_total`, `worker_request_duration_seconds`, `worker_kafka_produce_total`, `worker_jwt_operations_total`).

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8001` (`8000` in Docker/K8s) | HTTP listener port |
| `METRICS_PORT` | `9092` (`9091` in Docker/K8s) | Prometheus `/metrics` listener port |
| `KAFKA_BROKERS` | `localhost:9092` | Comma-separated Kafka bootstrap servers |
| `KAFKA_METADATA_TOPIC` | `device-metadata` | Kafka topic for `SystemStartupEvent` device metadata |
| `DEVICE_INDEX_PREFIX` | `mobile-devices` | OpenSearch index prefix for startup metadata events |
| `GATEWAY_PRIVATE_KEY_PATH` | `./private_key.pem` | Path to ECDSA P-256 private key PEM for signing JWTs and challenges |
| `ACCESS_JWT_TTL_SECONDS` | `86400` (`900` in Docker/K8s) | Lifespan of minted session access JWTs in seconds |
| `JWT_ISSUER` | `logger-gateway` | `iss` claim embedded in enrollment and access JWTs |
| `CONSUL_HTTP_ADDR` | `http://localhost:8500` | Consul HTTP agent address for service registration and KV sync |
| `ADVERTISED_IP` | `auto` | IP address advertised to Consul service discovery |
| `NODE_ID` | `auto` | Unique service instance identifier for Consul registration |
| `RUST_LOG` | `info` | Tracing filter directives |

---

## Testing

```bash
cd auth_worker
cargo test
```
