# Logger Gateway (`logger_gateway`)

A high-performance reverse proxy and entry boundary for the Remote Logger pipeline, built on Cloudflare's [Pingora](https://github.com/cloudflare/pingora) framework.

---

## Responsibilities

1. **Client Boundary & Proxying**:
   - Listens on `:8080` (default) for client HTTP/HTTPS requests.
   - Blindly proxies `/auth/*` routes (`GET /auth/challenge`, `POST /auth/enroll`, `POST /auth/exchange`) to `auth_worker` instances, preserving `X-Channel-Id` and request payloads.
   - Intercepts log ingestion paths (`/v1/logs/:channel` and `/v1/logs/:channel/batch`) and enforces `max_body_bytes` streaming size limits.
2. **ES256 JWT Authentication**:
   - Validates the `Authorization: Bearer <token>` header on ingestion routes against an ECDSA P-256 public key (`GATEWAY_PUBLIC_KEY_PATH`).
   - Verifies that `token_type == "access"`, `iss` matches the configured issuer, and the token is not expired.
   - Extracts verified identity and attestation claims (`sub`, `app`, `session_id`, `att`), strips the client `Authorization` header, and injects internal headers (`x-app-id`, `x-device-id`, `x-session-id`, `x-attestation`) for upstream `validation_worker` instances.
3. **Dynamic Upstream Discovery**:
   - Discovers healthy `auth_worker` (`/v1/health/service/auth_worker?passing=true`) and `validation_worker` (`/v1/health/service/validation_worker?passing=true`) instances via HashiCorp Consul.
   - Manages independent round-robin load balancers across healthy worker pools.
4. **Dynamic Configuration Sync**:
   - Watches `gateway/config/global` in Consul KV with HTTP blocking queries for zero-downtime reloads of `max_body_bytes` and `jwt.issuer`.
5. **Observability**:
   - Exports Prometheus metrics on `:9090` (`gateway_requests_total`, `gateway_request_duration_seconds`, `gateway_jwt_validations_total`, `gateway_body_size_bytes`, `gateway_envelope_errors_total`).

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `CONSUL_HTTP_ADDR` | `http://localhost:8500` | Consul agent endpoint |
| `GATEWAY_BIND_ADDR` | `0.0.0.0:8080` | Proxy HTTP listener address |
| `GATEWAY_METRICS_ADDR` | `0.0.0.0:9090` | Prometheus `/metrics` address |
| `GATEWAY_PUBLIC_KEY_PATH` | `./public_key.pem` | Path to ECDSA P-256 public key |
| `GATEWAY_TLS_CERT_PATH` | — | Path to optional TLS certificate |
| `GATEWAY_TLS_KEY_PATH` | — | Path to optional TLS private key |
| `RUST_LOG` | `info` | Tracing filter directive |

---

## Running Locally

```bash
cargo run --bin logger_gateway
```
