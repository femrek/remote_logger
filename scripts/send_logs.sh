#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status
set -euo pipefail

# Default values
TOKEN_FILE=".token"
GATEWAY_URL="http://localhost:8080"
CHANNEL="example-app-dev"
MESSAGE="Database connection established successfully."
LEVEL="INFO"
ZONE="main"

# Function to display usage information
usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Send a single test log to the Remote Logger Backend.

OPTIONS:
  -u <url>         Gateway base URL (default: ${GATEWAY_URL})
  -c <channel>     Channel slug to target (default: ${CHANNEL})
  -f <token_file>  Token file path (default: ${TOKEN_FILE})
  -m <message>     Message to log (default: "${MESSAGE}")
  -l <level>       Log level (default: ${LEVEL})
  -z <zone>        Log zone (default: ${ZONE})
  -h, --help       Show this help message and exit

EXAMPLES:
  $(basename "$0")
  $(basename "$0") -u http://localhost:8080 -c my-channel -m "Hello world" -l DEBUG
EOF
}

# Handle --help before getopts (getopts does not support long flags)
for arg in "$@"; do
  case "$arg" in
    --help) usage; exit 0 ;;
  esac
done

# Arg parsing
while getopts "u:c:f:m:l:z:h" opt; do
  case $opt in
    u) GATEWAY_URL="$OPTARG" ;;
    c) CHANNEL="$OPTARG" ;;
    f) TOKEN_FILE="$OPTARG" ;;
    m) MESSAGE="$OPTARG" ;;
    l) LEVEL="$OPTARG" ;;
    z) ZONE="$OPTARG" ;;
    h) usage; exit 0 ;;
    *) echo "Unknown option. Run '$(basename "$0") --help' for usage." >&2; exit 1 ;;
  esac
done

# Strip trailing slash from GATEWAY_URL if present
GATEWAY_URL="${GATEWAY_URL%/}"

# Function to get a new token
fetch_new_token() {
  echo "Fetching new auth token..."
  local response
  response=$(curl -s -X POST "${GATEWAY_URL}/auth/exchange" \
    -H "Content-Type: application/json" \
    -H "X-Channel-Id: ${CHANNEL}" \
    -d '{
      "platform": "android",
      "attestation_token": "mock",
      "tenant_id": "example_tenant",
      "device_uuid": "a809b022-7934-4b53-90d2-bf7cb95a947d",
      "device_metadata": {
        "timestamp": "'"$(date -u +'%Y-%m-%dT%H:%M:%SZ')"'",
        "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
        "sessionId": "3f338d38-9588-46d4-83b6-96d5395a12ef",
        "os": "Android",
        "osVersion": "13",
        "model": "Pixel 6",
        "brand": "Google",
        "device": "raven",
        "identifier": "build-1",
        "manufacturer": "Google",
        "isPhysicalDevice": "true"
      }
    }')

  # Extract token using jq or fallback grep/sed
  local token
  if command -v jq >/dev/null 2>&1; then
    token=$(echo "$response" | jq -r '.access_token')
  else
    token=$(echo "$response" | grep -o '"access_token":"[^"]*' | cut -d'"' -f4)
  fi

  if [ -z "${token}" ] || [ "${token}" = "null" ]; then
    echo "Error: Failed to obtain token from gateway. Response: ${response}" >&2
    exit 1
  fi

  echo "${token}" > "${TOKEN_FILE}"
  echo "New token saved to ${TOKEN_FILE}."
}

# Function to send logs
send_logs() {
  local token_to_use="$1"
  echo "Sending log request..." >&2
  
  # Create a temporary file to hold response body
  local response_file
  response_file=$(mktemp)
  
  local http_code
  http_code=$(curl -s -o "${response_file}" -w "%{http_code}" -X POST "${GATEWAY_URL}/v1/logs/${CHANNEL}" \
    -H "Authorization: Bearer ${token_to_use}" \
    -H "Content-Type: application/json" \
    -d '{
      "timestamp": "'"$(date -u +'%Y-%m-%dT%H:%M:%SZ')"'",
      "loggerName": "AppDatabase",
      "message": "'"${MESSAGE}"'",
      "level": "'"${LEVEL}"'",
      "zone": "'"${ZONE}"'",
      "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
      "sessionId": "3f338d38-9588-46d4-83b6-96d5395a12ef"
    }')
    
  local response_body
  response_body=$(cat "${response_file}")
  rm -f "${response_file}"
  
  echo "HTTP Status: ${http_code}" >&2
  echo "Response: ${response_body}" >&2
  
  echo "${http_code}"
}

# Main Execution Flow

# 1. Read token from file if it exists
TOKEN=""
if [ -f "${TOKEN_FILE}" ]; then
  TOKEN=$(cat "${TOKEN_FILE}")
fi

# 2. If no saved token, fetch one
if [ -z "${TOKEN}" ]; then
  fetch_new_token
  TOKEN=$(cat "${TOKEN_FILE}")
fi

# 3. Try sending the logs
STATUS_CODE=$(send_logs "${TOKEN}")

# 4. If status is 401 or 403 (unauthorized/expired), try to refresh token and retry
if [ "${STATUS_CODE}" = "401" ] || [ "${STATUS_CODE}" = "403" ]; then
  echo "Token expired or unauthorized (HTTP ${STATUS_CODE}). Requesting a new token..."
  fetch_new_token
  TOKEN=$(cat "${TOKEN_FILE}")
  STATUS_CODE=$(send_logs "${TOKEN}")
fi

# 5. Check final status code
if [[ "${STATUS_CODE}" -ge 200 && "${STATUS_CODE}" -lt 300 ]]; then
  echo "Log successfully sent!"
  exit 0
else
  echo "Error: Request failed with HTTP status code ${STATUS_CODE}" >&2
  exit 1
fi
