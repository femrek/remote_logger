#!/usr/bin/env bash

# Exit immediately if a command exits with a non-zero status
set -euo pipefail

TOKEN_FILE=".token"
GATEWAY_URL="http://localhost:8080"
CHANNEL="example-app-dev"

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
        "timestamp": "2026-06-23T15:00:00Z",
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
      "timestamp": "2026-06-23T15:00:00Z",
      "loggerName": "AppDatabase",
      "message": "Database connection established successfully.",
      "level": "INFO",
      "zone": "main",
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
