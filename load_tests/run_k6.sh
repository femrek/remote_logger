#!/usr/bin/env bash
# load_tests/run_k6.sh
#
# Docker wrapper script to run k6 step-by-step stress tests against the backend.
#
# Usage:
#   bash load_tests/run_k6.sh [OPTIONS] [K6_OPTIONS...]
#
# Options:
#   -u <url>       Gateway base URL (default: http://localhost:8080)
#   -s <channel>   Channel to target (default: example-app-dev)
#   -n <network>   Docker network to join (optional)
#   -q, --quick    Run a quick 20-second validation test (5 VUs max)
#   -h, --help     Show this help message
#
# Examples:
#   bash load_tests/run_k6.sh                       # runs step-by-step stress test against localhost
#   bash load_tests/run_k6.sh -u http://10.0.0.5:8080 # runs against remote dev deployment
#   bash load_tests/run_k6.sh -q                    # runs a quick local validation dry run

set -euo pipefail

# ── Setup path ────────────────────────────────────────────────────────────────
# Resolve the directory where this script resides to ensure correct volume mounts.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── Defaults ──────────────────────────────────────────────────────────────────
# ── Defaults ──────────────────────────────────────────────────────────────────
USER_GATEWAY_URL="${GATEWAY_URL:-}"
USER_DOCKER_NETWORK="${DOCKER_NETWORK:-UNSET}"
CHANNEL="${CHANNEL:-example-app-dev}"
QUICK_TEST="false"

# ── Help ──────────────────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS] [-- K6_OPTIONS...]

Run the step-by-step k6 load test in a Docker container.

OPTIONS:
  -u <url>       Gateway base URL. Defaults to the gateway container address
                 (http://logger_gateway:8080) if attached to a network,
                 or http://localhost:8080 if not.
  -s <channel>   Channel slug to target. Defaults to CHANNEL env var or example-app-dev.
  -n <network>   Attach the k6 container to an external Docker network. (default: logger_backend_logger_network)
                 Pass an empty string -n "" to run in default bridge network.
  -q, --quick    Run a quick 20-second validation test (5 VUs) instead of the full stress test.
  -h, --help     Show this help message.

K6_OPTIONS:
  Any arguments passed after '--' are forwarded directly to the k6 run command.
  Example: bash load_tests/run_k6.sh -- --http-debug="full"

EOF
}

# ── Arg parsing ───────────────────────────────────────────────────────────────
K6_ARGS=()
while [ $# -gt 0 ]; do
  case "$1" in
    -u)
      if [ -z "${2:-}" ]; then echo "Error: -u requires a value"; exit 1; fi
      USER_GATEWAY_URL="$2"
      shift 2
      ;;
    -s)
      if [ -z "${2:-}" ]; then echo "Error: -s requires a value"; exit 1; fi
      CHANNEL="$2"
      shift 2
      ;;
    -n)
      # Accept empty string to disable network joining
      USER_DOCKER_NETWORK="$2"
      shift 2
      ;;
    -q|--quick)
      QUICK_TEST="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      K6_ARGS=("$@")
      break
      ;;
    *)
      # Assume any unparsed args are k6 options if '--' wasn't used
      K6_ARGS+=("$1")
      shift
      ;;
  esac
done

# Resolve final network and URL values after parsing arguments
if [ "$USER_DOCKER_NETWORK" = "UNSET" ]; then
  DOCKER_NETWORK="logger_backend_logger_network"
else
  DOCKER_NETWORK="$USER_DOCKER_NETWORK"
fi

GATEWAY_URL="$USER_GATEWAY_URL"
if [ -z "$GATEWAY_URL" ]; then
  if [ -n "$DOCKER_NETWORK" ]; then
    GATEWAY_URL="http://logger_gateway:8080"
  else
    GATEWAY_URL="http://localhost:8080"
  fi
fi

# Strip trailing slash from URL
GATEWAY_URL="${GATEWAY_URL%/}"

# Print configuration summary
echo "╔══════════════════════════════════════════════╗"
echo "║   Remote Logger Backend — k6 Load Runner     ║"
echo "╠══════════════════════════════════════════════╣"
echo "║  Gateway Target: ${GATEWAY_URL}"
echo "║  Channel Slug  : ${CHANNEL}"
echo "║  Quick Mode    : ${QUICK_TEST}"
echo "║  Docker Network: ${DOCKER_NETWORK:-default (bridge)}"
if [ "${QUICK_TEST}" = "true" ]; then
  echo "║  Scenario      : 20s validation run"
else
  echo "║  Scenario      : Step-by-step stress test"
fi
echo "╚══════════════════════════════════════════════╝"
echo ""

# Helpful docker network tips for localhost targets
if [[ "$GATEWAY_URL" == *"localhost"* || "$GATEWAY_URL" == *"127.0.0.1"* ]] && [ -z "${DOCKER_NETWORK}" ]; then
  echo "⚠️  WARNING: You are targeting localhost inside a Docker container."
  echo "   If your backend is running locally on the host machine, k6 in Docker"
  echo "   won't be able to resolve localhost."
  echo "   - On macOS/Windows: run with '-u http://host.docker.internal:8080'"
  echo "   - On Linux: run with extra docker flag '--net=host' or joint network '-n <network>'"
  echo ""
fi

# ── Execute ───────────────────────────────────────────────────────────────────
echo "► Pulling latest grafana/k6 Docker image..."
docker pull grafana/k6:latest

echo "► Launching k6 stress test..."
# Run docker with terminal interactive mode if stdout is a TTY
DOCKER_FLAGS=("-i" "--rm")
if [ -t 0 ] && [ -t 1 ]; then
  DOCKER_FLAGS+=("-t")
fi

if [ -n "${DOCKER_NETWORK}" ]; then
  DOCKER_FLAGS+=("--network" "${DOCKER_NETWORK}")
fi

docker run "${DOCKER_FLAGS[@]}" \
  -v "${ROOT_DIR}/load_tests:/load_tests:ro" \
  -e GATEWAY_URL="${GATEWAY_URL}" \
  -e CHANNEL="${CHANNEL}" \
  -e QUICK_TEST="${QUICK_TEST}" \
  grafana/k6:latest \
  run /load_tests/stress_test.js ${K6_ARGS[@]+"${K6_ARGS[@]}"}
