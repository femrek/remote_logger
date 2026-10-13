# Logger Gateway (`logger_gateway`)

A high-performance reverse proxy and entry boundary for the Remote Logger pipeline, built on Cloudflare's [Pingora](https://github.com/cloudflare/pingora) framework.

---

## Responsibilities

1. **Client Boundary & Proxying**:
   - Listens on `:8080` (default) for client requests.
   - Blindly proxies `/auth/exchange` to validation workers.
   - Intercepts log ingestion paths (`/v1/logs/:channel` and `/v1/logs/:channel/batch`).
2. **ES256 JWT Authentication**:
   - Validates the `Authorization: Bearer <token>` header against an ECDSA P-256 public key (`GATEWAY_PUBLIC_KEY_PATH`).
   - Extracts verified device identity from `claims.sub`.
   - Strips client `Authorization` headers and injects internal identity headers (`x-app-id`, `x-device-id`, `x-session-id`) for upstream services.
3. **Dynamic Upstream Discovery**:
   - Discovers healthy `validation_worker` instances via HashiCorp Consul (`/v1/health/service/validation_worker?passing=true`).
   - Manages round-robin load balancing across healthy worker instances.
4. **Dynamic Configuration Sync**:
   - Watches `gateway/config/global` in Consul KV with HTTP blocking queries for zero-downtime reloads.
5. **Observability**:
   - Exports Prometheus metrics on `:9090` (`gateway_requests_total`, `gateway_request_duration_seconds`, `gateway_jwt_validations_total`).

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
