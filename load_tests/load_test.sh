#!/usr/bin/env bash
# load_tests/load_test.sh
#
# Simple load tester for the Remote Logger Backend.
# Sends concurrent log requests to generate real traffic visible in Grafana.
#
# Usage:
#   bash load_tests/load_test.sh [OPTIONS]
#
# Options:
#   -n <number>   Total number of requests to send (default: 200)
#   -c <number>   Concurrency / parallel workers (default: 10)
#   -u <url>      Gateway base URL (default: http://localhost:8080)
#   -s <channel>  Channel to target (default: example-app-dev)
#
# Prerequisites:
#   - Stack must be running: cd docker && docker compose up -d
#   - curl must be installed
#   - Optional: jq (for prettier output)
#
# Examples:
#   bash load_tests/load_test.sh                   # 200 requests, 10 concurrent
#   bash load_tests/load_test.sh -n 1000 -c 20    # 1000 requests, 20 concurrent
#   bash load_tests/load_test.sh -n 50 -c 5 -s example-app-prod

set -euo pipefail

# ── Defaults ────────────────────────────────────────────────────────────────────
TOTAL=200
CONCURRENCY=10
DURATION=0          # 0 = disabled; >0 = run for this many seconds (-t flag)
GATEWAY_URL="http://localhost:8080"
CHANNEL="example-app-dev"
TOKEN_FILE=".token"

# ── Help ────────────────────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Send concurrent log requests to the Remote Logger Backend to generate
real traffic visible in Grafana dashboards.

OPTIONS:
  -n <number>    Total number of requests to send     (default: ${TOTAL})
  -t <seconds>   Run continuously for this duration   (overrides -n)
  -c <number>    Concurrency — parallel workers        (default: ${CONCURRENCY})
  -u <url>       Gateway base URL                      (default: ${GATEWAY_URL})
  -s <channel>   Channel slug to target                (default: ${CHANNEL})
  -h, --help     Show this help message and exit

MODES:
  Fixed count (-n)    Send exactly N requests then print summary.
  Duration  (-t)      Keep firing batches of -c requests until time is up.
                      Live progress is printed after each batch.
                      Press Ctrl+C to stop early — summary is still shown.

PREREQUISITES:
  • Stack must be running:  cd docker && docker compose up -d
  • curl must be installed
  • jq is optional (used for nicer JSON parsing)

EXAMPLES:
  $(basename "$0")                           # 200 requests, 10 concurrent
  $(basename "$0") -n 1000 -c 20            # fixed count, heavier load
  $(basename "$0") -t 60                    # run for 60 seconds
  $(basename "$0") -t 300 -c 20            # run for 5 minutes, 20 concurrent
  $(basename "$0") -t 60 -s example-app-prod
  $(basename "$0") -u http://staging:8080   # point at a remote gateway

OUTPUT:
  After the run, open Grafana at http://localhost:3000
  Dashboard: Logger Backend — Full Stack
  Set time range to 'Last 5 minutes' to see the traffic.
EOF
}

# Handle --help before getopts (getopts does not support long flags)
for arg in "$@"; do
  case "$arg" in
    --help) usage; exit 0 ;;
  esac
done

# ── Arg parsing ─────────────────────────────────────────────────────────────────
while getopts "n:t:c:u:s:h" opt; do
  case $opt in
    n) TOTAL="$OPTARG" ;;
    t) DURATION="$OPTARG" ;;
    c) CONCURRENCY="$OPTARG" ;;
    u) GATEWAY_URL="$OPTARG" ;;
    s) CHANNEL="$OPTARG" ;;
    h) usage; exit 0 ;;
    *) echo "Unknown option. Run '$(basename "$0") --help' for usage."; exit 1 ;;
  esac
done

# Strip trailing slash from GATEWAY_URL if present
GATEWAY_URL="${GATEWAY_URL%/}"

# ── Token acquisition ───────────────────────────────────────────────────────────
get_token() {
  local response
  response=$(curl -sf -X POST "${GATEWAY_URL}/auth/exchange" \
    -H "Content-Type: application/json" \
    -H "X-Channel-Id: ${CHANNEL}" \
    -d '{
      "platform": "android",
      "attestation_token": "mock",
      "tenant_id": "load_test_tenant",
      "device_id": "b7d2a1e0-0000-4000-a000-000000000001",
      "device_metadata": {
        "timestamp": "'"$(date -u +'%Y-%m-%dT%H:%M:%SZ')"'",
        "deviceId": "b7d2a1e0-0000-4000-a000-000000000001",
        "sessionId": "load-test-session",
        "os": "Android",
        "osVersion": "14",
        "model": "Load Test Device",
        "brand": "TestBrand",
        "device": "loadtest",
        "identifier": "LT-001",
        "manufacturer": "TestCorp",
        "isPhysicalDevice": "false"
      }
    }') || { echo "ERROR: Auth exchange failed. Is the stack running? (cd docker && docker compose up -d)"; exit 1; }

  if command -v jq >/dev/null 2>&1; then
    echo "$response" | jq -r '.access_token'
  else
    echo "$response" | grep -o '"access_token":"[^"]*' | cut -d'"' -f4
  fi
}

# ── Single request sender ───────────────────────────────────────────────────────
send_one() {
  local token="$1"
  local idx="$2"
  local level
  # Cycle through log levels to generate variety in the data
  case $(( idx % 5 )) in
    0) level="INFO"    ;;
    1) level="DEBUG"   ;;
    2) level="WARN"    ;;
    3) level="ERROR"   ;;
    4) level="SEVERE"  ;;
  esac

  local http_code
  http_code=$(curl -sf -o /dev/null -w "%{http_code}" \
    -X POST "${GATEWAY_URL}/v1/logs/${CHANNEL}" \
    -H "Authorization: Bearer ${token}" \
    -H "Content-Type: application/json" \
    -d '{
      "timestamp":  "'"$(date -u +'%Y-%m-%dT%H:%M:%SZ')"'",
      "loggerName": "LoadTest.Worker'"${idx}"'",
      "message":    "Load test request '"${idx}"' — stress testing the pipeline",
      "level":      "'"${level}"'",
      "zone":       "load-test",
      "deviceId":   "b7d2a1e0-0000-4000-a000-'"$(printf '%012d' "$idx")"'",
      "sessionId":  "load-test-'"${idx}"'"
    }' 2>/dev/null) || http_code="ERR"
  echo "$http_code"
}

export -f send_one

# ── Main ─────────────────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║   Remote Logger Backend — Load Test          ║"
echo "╠══════════════════════════════════════════════╣"
echo "║  Gateway    : ${GATEWAY_URL}"
echo "║  Channel    : ${CHANNEL}"
if [ "$DURATION" -gt 0 ] 2>/dev/null; then
  echo "║  Mode       : duration (${DURATION}s)"
else
  echo "║  Mode       : fixed count (${TOTAL} requests)"
fi
echo "║  Concurrency: ${CONCURRENCY}"
echo "╚══════════════════════════════════════════════╝"
echo ""

# Step 1: Get token
echo "► Acquiring auth token..."
TOKEN=$(get_token)
if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
  echo "ERROR: Could not extract access_token from response."
  exit 1
fi
echo "  Token acquired ✓"
echo ""

# Step 2: Warm up (1 request to confirm it works)
echo "► Warm-up request..."
WARMUP=$(send_one "$TOKEN" 0)
if [ "$WARMUP" != "200" ]; then
  echo "  ERROR: Warm-up returned HTTP ${WARMUP}. Check gateway logs."
  echo "  Run: cd docker && docker compose logs logger_gateway"
  exit 1
fi
echo "  Warm-up OK (HTTP 200) ✓"
echo ""

# Step 3a: Build the shared worker script (used in both modes)
RESULTS_DIR=$(mktemp -d)
WORKER_SCRIPT=$(mktemp /tmp/lt_worker.XXXXXX.sh)
cat > "$WORKER_SCRIPT" <<WORKER
#!/usr/bin/env bash
TOKEN="\$1"
RAW_IDX="\$2"          # may be "BATCH_SLOT" in duration mode
RESULTS_DIR="\$3"
GATEWAY_URL="${GATEWAY_URL}"
CHANNEL="${CHANNEL}"

# Strip any batch prefix (e.g. "7_3" → "3") so arithmetic always works
IDX="\${RAW_IDX##*_}"

case \$(( IDX % 5 )) in
  0) LEVEL="INFO"   ;;
  1) LEVEL="DEBUG"  ;;
  2) LEVEL="WARN"   ;;
  3) LEVEL="ERROR"   ;;
  4) LEVEL="SEVERE" ;;
esac

CODE=\$(curl -sf -o /dev/null -w "%{http_code}" \\
  -X POST "\${GATEWAY_URL}/v1/logs/\${CHANNEL}" \\
  -H "Authorization: Bearer \${TOKEN}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "timestamp":  "'\$(date -u +"%Y-%m-%dT%H:%M:%SZ")'",
    "loggerName": "LoadTest.Worker'"\${IDX}"'",
    "message":    "Load test request '"\${IDX}"'",
    "level":      "'\${LEVEL}'",
    "zone":       "load-test",
    "deviceId": "b7d2a1e0-0000-4000-a000-'\$(printf '%012d' "\${IDX}")'",
    "sessionId":  "lt-'"\${IDX}"'"
  }' 2>/dev/null) || CODE="ERR"

echo "\$CODE" > "\${RESULTS_DIR}/\${RAW_IDX}"
WORKER
chmod +x "$WORKER_SCRIPT"

# Cleanup on Ctrl+C so the summary still prints
SUCCESS=0; FAIL=0; TOTAL_SENT=0
print_summary() {
  local elapsed=$(( $(date +%s) - START_TS ))
  local rps=$(( TOTAL_SENT / (elapsed > 0 ? elapsed : 1) ))
  echo ""
  echo "╔══════════════════════════════════════════════╗"
  echo "║   Results                                    ║"
  echo "╠══════════════════════════════════════════════╣"
  printf "║  %-14s %d\n" "Sent:"         "$TOTAL_SENT"
  printf "║  %-14s %d\n" "Success (200):" "$SUCCESS"
  printf "║  %-14s %d\n" "Failed:"       "$FAIL"
  printf "║  %-14s %ds\n" "Duration:"    "$elapsed"
  printf "║  %-14s ~%d req/s\n" "Throughput:" "$rps"
  echo "╚══════════════════════════════════════════════╝"
  echo ""
  echo "► Open Grafana to see the traffic: http://localhost:3000"
  echo "  Dashboard: Logger Backend — Full Stack"
  echo "  (Set time range to 'Last 5 minutes' if graphs appear empty)"
  echo ""
}

count_results() {
  for f in "$RESULTS_DIR"/*; do
    [ -f "$f" ] || continue
    code=$(cat "$f")
    rm -f "$f"
    if [ "$code" = "200" ]; then
      (( SUCCESS++ )) || true
    else
      (( FAIL++ )) || true
    fi
    (( TOTAL_SENT++ )) || true
  done
}

trap 'echo ""; echo "Interrupted — tallying results..."; count_results; rm -f "$WORKER_SCRIPT"; rm -rf "$RESULTS_DIR"; print_summary; exit 0' INT TERM

START_TS=$(date +%s)

# ─────────────────────────────────────────────────────────────────────────────
# Step 3b: Execute in the selected mode
# ─────────────────────────────────────────────────────────────────────────────
if [ "$DURATION" -gt 0 ] 2>/dev/null; then
  # ── DURATION MODE: fire batches until time is up ──────────────────────────────
  DEADLINE=$(( START_TS + DURATION ))
  BATCH=0
  echo "► Running for ${DURATION}s — press Ctrl+C to stop early"
  echo ""
  while [ "$(date +%s)" -lt "$DEADLINE" ]; do
    (( BATCH++ )) || true
    BATCH_START=$(date +%s)
    seq 1 "$CONCURRENCY" | xargs -P"$CONCURRENCY" -I{} \
      bash "$WORKER_SCRIPT" "$TOKEN" "${BATCH}_{}" "$RESULTS_DIR"
    count_results
    REMAINING=$(( DEADLINE - $(date +%s) ))
    ELAPSED_SO_FAR=$(( $(date +%s) - START_TS ))
    BATCH_RPS=$(( CONCURRENCY / ( ($(date +%s) - BATCH_START) > 0 ? ($(date +%s) - BATCH_START) : 1 ) ))
    printf "  batch %-4d | sent %-6d | ok %-6d | fail %-4d | %ds remaining\r" \
      "$BATCH" "$TOTAL_SENT" "$SUCCESS" "$FAIL" "$REMAINING"
  done
  echo ""  # newline after the \r progress line
else
  # ── FIXED COUNT MODE: send exactly TOTAL requests ────────────────────────────
  echo "► Starting load test (${TOTAL} requests, ${CONCURRENCY} concurrent)..."
  seq 1 "$TOTAL" | xargs -P"$CONCURRENCY" -I{} bash "$WORKER_SCRIPT" "$TOKEN" {} "$RESULTS_DIR"
  count_results
fi

rm -f "$WORKER_SCRIPT"
rm -rf "$RESULTS_DIR"

print_summary
