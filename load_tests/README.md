# Remote Logger Backend — Load Testing Suite

This directory contains the tools and scripts used to load-test and stress-test the Remote Logger pipeline, check system limitations, and identify breaking points.

---

## Directory Contents

| File | Type | Description |
|---|---|---|
| [`stress_test.js`](file:///Users/femrek/projects/exclusive/remote_logger_backend/load_tests/stress_test.js) | k6 (JavaScript) | Stress testing logic defining a step-by-step VU ramp-up scenario, checks, and JWT token lifecycle management. |
| [`run_k6.sh`](file:///Users/femrek/projects/exclusive/remote_logger_backend/load_tests/run_k6.sh) | Bash Script | Docker runner wrapper for the k6 stress test. Pulls and executes the official `grafana/k6` container image. |
| [`load_test.sh`](file:///Users/femrek/projects/exclusive/remote_logger_backend/load_tests/load_test.sh) | Bash Script | Lightweight, dependency-free bash curl parallel runner. Good for quick smoke testing and low concurrency. |

---

## 1. Advanced k6 Step-by-Step Stress Test (Recommended)

This tool is designed to find system limits by increasing virtual users (VUs) step-by-step. 

### Step Load Scenario
By default, the script executes the following stages:
1. **0 to 10 VUs** over 30s, hold at **10 VUs** for 1m (Warm-up / Baseline load).
2. **10 to 50 VUs** over 30s, hold at **50 VUs** for 2m (Normal operational load).
3. **50 to 150 VUs** over 30s, hold at **150 VUs** for 2m (High load).
4. **150 to 300 VUs** over 1m, hold at **300 VUs** for 3m (Stress limit boundary).
5. **300 to 600 VUs** over 1m, hold at **600 VUs** for 3m (System breaking point).
6. **600 to 0 VUs** over 1m (System recovery observation).

*Note: Each VU sleeps for 100ms between log requests to simulate realistic client think-time, resulting in a maximum throughput capability of ~10 requests/sec per active VU (up to 6,000 req/s at peak).*

### How to Run

### How to Run

Running the stress test requires only **Docker** to be installed:

```bash
# Run against the local stack (joins the default docker network 'logger_backend_logger_network' automatically)
bash load_tests/run_k6.sh

# Run against a remote dev deployment (bypasses default docker network joining)
bash load_tests/run_k6.sh -n "" -u "http://<YOUR_DEV_IP_OR_DOMAIN>:8080"
```

### Script CLI Flags

```bash
# Show usage help
bash load_tests/run_k6.sh -h

# Target a custom channel slug (default is example-app-dev)
bash load_tests/run_k6.sh -s example-app-prod

# Attach to a specific custom Docker network
bash load_tests/run_k6.sh -n "my_custom_network"

# Disable network attachment (runs in standard bridge mode)
bash load_tests/run_k6.sh -n ""

# Run a quick 20-second validation run (max 5 VUs) to test connection and credentials
bash load_tests/run_k6.sh -q

# Pass custom options directly to k6 (separated by '--')
bash load_tests/run_k6.sh -- --http-debug="full"
```

### Local Docker Network & Loopback
By default, the script attaches to `logger_backend_logger_network` and uses the gateway container address `http://logger_gateway:8080`.
If you disable the network with `-n ""`, the target defaults to `http://localhost:8080`. Remember that `localhost` inside the k6 container refers to the container itself. In that case:
- On macOS/Windows: target the host loopback using `-u http://host.docker.internal:8080`
- On Linux: append `-- --net=host` to share the host's networking namespace.

---

## 2. Lightweight Bash Load Tester

A quick script that does not require Docker or k6. It runs simple parallel curls via `xargs`.

```bash
# Run 200 total requests with 10 concurrency (default)
bash load_tests/load_test.sh

# Send 1000 total requests with 20 concurrent threads
bash load_tests/load_test.sh -n 1000 -c 20

# Run continuously for 60 seconds (duration mode)
bash load_tests/load_test.sh -t 60
```

---

## Finding System Limits: Metrics to Monitor

Open your deployment's **Grafana Dashboard** (`http://localhost:3000` locally) and monitor the following indicators during the step load test to detect system bottlenecks:

1. **HTTP Ingestion Latency (`http_req_duration`)**:
   - Check the `p(95)` and `p(99)` latency. Sizable spikes (e.g. latency jumping from ~10ms to >500ms) show backend queue saturation.
2. **HTTP Error Rate (`http_req_failed`)**:
   - Saturated worker or gateway services will trigger connection timeouts or 502/503 bad gateway codes. A threshold of >2% failures indicates system breaking point.
3. **Kafka Consumer Group Lag**:
   - If log payloads are successfully received but OpenSearch indexing is bottlenecked, logs will accumulate in the Kafka `mobile-logs` topic. Watch Grafana's **Kafka Lag** gauge; if it keeps climbing, OpenSearch is the bottleneck.
4. **OpenSearch Indexing Latency**:
   - High bulk-write queue wait times or heavy CPU utilization on the OpenSearch container indicates index writing congestion.
