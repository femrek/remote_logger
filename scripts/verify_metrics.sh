#!/usr/bin/env bash
# scripts/verify_metrics.sh
#
# Smoke-tests every Prometheus /metrics endpoint exposed by the stack.
# Requires the full docker compose stack to be running.
#
# Usage:
#   cd docker && docker compose up -d
#   ../scripts/verify_metrics.sh

set -euo pipefail

PASS=0
FAIL=0

check() {
    local name="$1"
    local url="$2"
    local metric="$3"

    echo -n "  [$name] $url ... "
    if curl -sf "$url" 2>/dev/null | grep -q "$metric"; then
        echo "PASS"
        ((PASS++))
    else
        echo "FAIL (metric '$metric' not found)"
        ((FAIL++))
    fi
}

echo ""
echo "========================================="
echo " Remote Logger Backend — Metrics Check"
echo "========================================="
echo ""

echo ">> Gateway (Pingora) :9095"
check "gateway_requests_total"    "http://localhost:9095/metrics" "gateway_requests_total"
check "gateway_request_duration"  "http://localhost:9095/metrics" "gateway_request_duration_seconds"
check "gateway_jwt_validations"   "http://localhost:9095/metrics" "gateway_jwt_validations_total"

echo ""
echo ">> Validation Worker :9091"
check "worker_requests_total"     "http://localhost:9091/metrics" "worker_requests_total"
check "worker_kafka_produce"      "http://localhost:9091/metrics" "worker_kafka_produce_total"
check "worker_jwt_operations"     "http://localhost:9091/metrics" "worker_jwt_operations_total"

echo ""
echo ">> Prometheus UI :9091 (host port)"
check "prometheus_self"           "http://localhost:9091/metrics" "prometheus_build_info"

echo ""
echo ">> Kafka Exporter (via Prometheus scrape)"
check "kafka_exporter"            "http://localhost:9091/metrics" "kafka_brokers"

echo ""
echo ">> OpenSearch Exporter (via Prometheus scrape)"
check "opensearch_health"         "http://localhost:9091/metrics" "elasticsearch_cluster_health_status"

echo ""
echo ">> Fluent Bit :2020"
check "fluentbit_input"           "http://localhost:2020/api/v1/metrics/prometheus" "fluentbit_input_records_total"

echo ""
echo "========================================="
if [ "$FAIL" -eq 0 ]; then
    echo " ALL CHECKS PASSED ($PASS/$((PASS+FAIL)))"
    echo "========================================="
    exit 0
else
    echo " $FAIL CHECK(S) FAILED ($PASS/$((PASS+FAIL)) passed)"
    echo "========================================="
    exit 1
fi
