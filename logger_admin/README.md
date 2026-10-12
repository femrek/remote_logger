# Logger Admin (`logger_admin`)

An administration dashboard and REST API microservice for configuring the Remote Logger pipeline, built with [Axum](https://github.com/tokio-rs/axum) and HashiCorp Consul.

---

## Responsibilities

1. **Web Dashboard**:
   - Single-Page Application (vanilla JS/HTML/CSS embedded via `rust-embed`) served on port `8082`.
   - Visual interface for inspecting, adding, editing, and deleting application channels.
2. **Channel & Attestation Management**:
   - Manages routing rules and attestation enforcement stored in Consul KV under `gateway/config/channels/{id}`.
   - Updates global gateway parameters (`gateway/config/global`).
3. **Protected Admin API**:
   - Protected by `ADMIN_TOKEN` Bearer authentication.
   - Endpoints:
     - `GET /api/config`: Retrieves global settings and list of channels.
     - `POST /api/config`: Atomically writes global and channel configurations with CAS checks.
     - `DELETE /api/config/channels/{id}`: Deletes a specific channel.
     - `GET /health`: Health check endpoint.

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `8082` | HTTP listener port |
| `CONSUL_HTTP_ADDR` | `http://localhost:8500` | Consul agent endpoint |
| `ADMIN_TOKEN` | — | Required Bearer token for accessing admin APIs and UI |
| `NODE_ID` | Fresh UUIDv4 | Unique node identifier for Consul registration |
| `RUST_LOG` | `info` | Tracing filter directive |

---

## Running Locally

```bash
ADMIN_TOKEN=my-secret-admin-token cargo run --bin logger_admin
```

Open `http://localhost:8082` in your browser and enter `my-secret-admin-token`.
