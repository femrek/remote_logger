import http from 'k6/http';
import { check, sleep } from 'k6';

// ── Configuration ─────────────────────────────────────────────────────────────
// Read settings from environment variables with sensible defaults.
const GATEWAY_URL = (__ENV.GATEWAY_URL || 'http://localhost:8080').replace(/\/$/, '');
const CHANNEL = __ENV.CHANNEL || 'example-app-dev';
const QUICK_TEST = __ENV.QUICK_TEST === 'true';

// Define step-by-step load test stages
let testStages = [
  { duration: '30s', target: 10 },  // Stage 1: Ramp up to 10 VUs
  { duration: '1m', target: 10 },   // Stage 2: Hold at 10 VUs (Warm-up / Baseline)
  { duration: '30s', target: 50 },  // Stage 3: Ramp up to 50 VUs
  { duration: '2m', target: 50 },   // Stage 4: Hold at 50 VUs (Normal Operational Load)
  { duration: '30s', target: 150 }, // Stage 5: Ramp up to 150 VUs
  { duration: '2m', target: 150 },  // Stage 6: Hold at 150 VUs (High Load)
  { duration: '1m', target: 300 },  // Stage 7: Ramp up to 300 VUs
  { duration: '3m', target: 300 },  // Stage 8: Hold at 300 VUs (Stress Limit Boundary)
  { duration: '1m', target: 600 },  // Stage 9: Ramp up to 600 VUs
  { duration: '3m', target: 600 },  // Stage 10: Hold at 600 VUs (Finding the breaking point)
  { duration: '1m', target: 0 },    // Stage 11: Ramp down to 0 VUs (Recovery phase)
];

// If QUICK_TEST is enabled, override stages with a minimal setup for validation
if (QUICK_TEST) {
  testStages = [
    { duration: '5s', target: 5 },
    { duration: '10s', target: 5 },
    { duration: '5s', target: 0 },
  ];
}

export const options = {
  stages: testStages,
  thresholds: {
    // Fail the test if more than 2% of the ingest requests fail
    http_req_failed: ['rate<0.02'],
    // 95% of request durations should be under 500ms
    http_req_duration: ['p(95)<500'],
  },
};

// ── VU-Local Caching ──────────────────────────────────────────────────────────
// Each virtual user (VU) runs in its own JS engine context. Variables declared
// here act as a local cache for the lifetime of that VU.
let cachedToken = null;
let tokenAcquiredAt = 0;
const TOKEN_REFRESH_INTERVAL = 720; // 12 minutes (safe margin before 15m expiration)

// Helper to exchange attestation token for a Bearer JWT session token
function getAuthToken(vuId) {
  const url = `${GATEWAY_URL}/auth/exchange`;
  const deviceId = `b7d2a1e0-0000-4000-a000-${String(vuId).padStart(12, '0')}`;
  
  const payload = JSON.stringify({
    platform: 'android',
    attestation_token: 'mock',
    tenant_id: 'load_test_tenant',
    device_id: deviceId,
    device_metadata: {
      timestamp: new Date().toISOString(),
      deviceId: deviceId,
      sessionId: `session-${vuId}`,
      os: 'Android',
      osVersion: '14',
      model: 'Load Test Device',
      brand: 'TestBrand',
      device: 'loadtest',
      identifier: `LT-${vuId}`,
      manufacturer: 'TestCorp',
      isPhysicalDevice: 'false'
    }
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
      'X-Channel-Id': CHANNEL,
    },
  };

  const response = http.post(url, payload, params);
  
  const ok = check(response, {
    'auth token exchange succeeded': (res) => res.status === 200,
  });

  if (!ok) {
    console.error(`VU ${vuId} failed to authenticate. Status: ${response.status}. Body: ${response.body}`);
    return null;
  }

  try {
    const json = JSON.parse(response.body);
    return json.access_token;
  } catch (err) {
    console.error(`VU ${vuId} failed to parse auth JSON: ${err}. Body: ${response.body}`);
    return null;
  }
}

// ── Main Scenario ─────────────────────────────────────────────────────────────
export default function () {
  const now = Date.now() / 1000;
  
  // 1. Ensure the VU has a valid session token
  if (!cachedToken || (now - tokenAcquiredAt) > TOKEN_REFRESH_INTERVAL) {
    cachedToken = getAuthToken(__VU);
    if (!cachedToken) {
      sleep(1); // Wait before retrying if auth failed
      return;
    }
    tokenAcquiredAt = now;
  }

  // 2. Cycle through different log levels to generate realistic data diversity
  const levels = ['INFO', 'DEBUG', 'WARN', 'ERROR', 'SEVERE'];
  const level = levels[(__ITER + __VU) % levels.length];

  const ingestUrl = `${GATEWAY_URL}/v1/logs/${CHANNEL}`;
  const deviceId = `b7d2a1e0-0000-4000-a000-${String(__VU).padStart(12, '0')}`;
  
  const payload = JSON.stringify({
    timestamp: new Date().toISOString(),
    loggerName: `k6.VU${__VU}.Iter${__ITER}`,
    message: `k6 stress test event. VU: ${__VU}, Iter: ${__ITER}, Level: ${level}`,
    level: level,
    zone: 'k6-stress-test',
    deviceId: deviceId,
    sessionId: `k6-session-${__VU}-${__ITER}`
  });

  const params = {
    headers: {
      'Authorization': `Bearer ${cachedToken}`,
      'Content-Type': 'application/json',
    },
  };

  // 3. Fire log event and assert success
  const response = http.post(ingestUrl, payload, params);

  check(response, {
    'log ingestion succeeded (HTTP 200)': (res) => res.status === 200,
  });

  // 4. Subtle sleep to emulate application logging intervals (think time)
  // 0.1s sleep translates to max ~10 requests/sec per active VU,
  // preventing a single VU from overloading the client/network loop instantly.
  sleep(0.1);
}
