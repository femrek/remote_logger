# Review Backlog: Attestation & Ingestion

Findings and suggestions from the senior mobile/backend review (2026-10-14) that are **not solved** by the current implementation plan (*Hardware Attestation, Refresh Removal & Session Integrity — Rev. 3*).

Each item says why it's open, what the risk is, and a suggested path forward.

Legend:
- **Severity:** 🔴 high · 🟠 medium · 🟡 low
- **Type:** `security` · `reliability` · `product` · `dx` · `ops`

---

## A. Partially solved review findings

### A1. Bearer token replay & rate limiting — 🟠 `security`
- **Finding (#6):** The 24h access JWT is a bearer token. On a rooted or proxied device, a user can extract it and send logs from any machine until it expires.
- **Decision:** **No rate limits for now** (per user decision). Gateway rate limiting is deferred and not implemented in the initial release.
- **Future Options:**
  1. Token-bucket rate limiting per `session_id` at `logger_gateway` (in-process or Redis-backed).
  2. Per-batch proof-of-possession (DPoP-style signature with the hardware key on each batch).
  3. Shorter access token TTL (e.g. 1–4h).

### A2. Leaked Android keyboxes that are not yet revoked — 🟠 `security`
- **Finding (#4):** Leaked OEM keyboxes often stay unrevoked for days or weeks. With one, a rooted device or emulator can produce hardware keystore attestations that look valid.
- **Suggestions:**
  - Periodically fetch and check Google's Keybox revocation list (`https://android.googleapis.com/attestation/status`).
  - Track a per-certificate-serial enrollment rate. One intermediate certificate behind thousands of "devices" is a leak signal.

### A3. iOS App Attest entitlement is a manual host-app step — 🟡 `dx`
- **Finding (#5):** A plugin can't add the App Attest capability. If the host app forgets it, `attestKey` fails at runtime.
- **What the plan does:** ~~documents the step in the README~~ (implemented in `mobile/flutter_remote_logging/README.md` & `docs/attestation.md`), and the backend checks AAGUID per channel in `AppleAppAttestVerifier` (`auth_worker`). When `apple_team_id` is omitted, `AppleAppAttestVerifier` also supports Secure Enclave P-256 signature verification.
- **Suggestions:**
  - Have the plugin detect the missing entitlement and emit **one** clear `internalLog.severe` message with setup instructions.
  - ~~Add a `doctor`-style check to the example app.~~ *(Implemented in `mobile/example` via `AttestationTab` and `DiagnosticController`)*
  - Report "attestation unavailable: entitlement missing" as a distinct error code to the backend so it's visible in dashboards.

### A4. Non-GMS Android devices (Huawei, some Chinese OEMs, AOSP builds) — 🟡 `product`
- **Finding (#7):** Devices without Google Mobile Services attest to OEM vendor roots (e.g. Huawei), not Google's.
- **Decision:** ~~**Treat as `att=none` for now** (per user decision). In `monitor` and `off` modes, logs from these devices are accepted normally with the `att=none` trust tag.~~ *(Implemented)*
- **Future Options:** Add support for OEM vendor-specific root certificate sets (e.g. Huawei Key Attestation Root CA) configurable per channel in Consul KV.

---

## B. Known limitations of the chosen design

### B1. Stateless challenge is not single-use — 🟡 `security`
- An HMAC challenge can be reused within its 5-minute TTL. Note that [`create_challenge`](../auth_worker/src/auth/jwt.rs) currently signs `nonce:exp:private_key` (not bound to `channel_id`), and [`prove`](../mobile/flutter_remote_logging/android/src/main/kotlin/dev/femrek/flutter_remote_logging/KeyAttestationPlugin.kt) signs `challenge` bytes directly (ignoring `sessionId` and `deviceId` parameters), though `/auth/exchange` verifies that the enrollment token's `app`, `sub` (`device_id`), and `plt` match the request.
- **If needed:** bind `channel_id` into `create_challenge`, sign `challenge:session_id:device_id` in `prove()`, and keep a short-lived seen-nonce set (in-memory LRU per `auth_worker` replica, or Redis for exact enforcement).

### B2. iOS assertion counter not checked — 🟡 `security`
- App Attest assertions include a monotonic counter. Enforcing it requires storing per-key state on the server, which conflicts with the stateless design. Freshness relies on the challenge (see B1).

### B3. Compromise after enrollment — 🟡 `security`
- An enrollment token stays valid for 30–90 days. If the device is rooted or jailbroken after enrollment, we won't notice until it re-enrolls.
- **Mitigations:** shorter enrollment TTL; check `RootOfTrust` at enrollment.

### B4. Runtime hooking (Frida, Xposed, jailbreak tweaks) — 🟡 `security`
- Attestation proves the key is in hardware and the app is genuine at enrollment. It can't stop a hooked app on a compromised device from signing fake data. This limit is inherent to client attestation, so we accept it.

### B5. In-process rate limiter across replicas — 🟡 `ops`
- The plan's per-session token bucket lives in each `logger_gateway` process. With N replicas, the effective limit is up to N× the configured one.
- **Options:** consistent-hash routing by `session_id`, or a shared limiter (Redis/Valkey `CL.THROTTLE`).

---

## C. Other issues & TODOs found in the codebase

### C1. Duplicate logs on retry (no idempotency) — 🟠 `reliability`
- [`ServerLog.toJsonPayload`](../mobile/remote_logging/lib/src/model/server_log.dart) **does not send the client `id`** ("Only for saving local database").
- [`batch_ingest_handler`](../validation_worker/src/handler/ingest.rs) sends to Kafka item by item. If item *k* fails, it returns 503 after items 0..k-1 were already produced. The client then retries the whole batch, which creates **duplicates** in OpenSearch.
- Offline replay after an ambiguous network error (request delivered, response lost) also duplicates.
- **Suggestions:**
  - Send `id` in the payload.
  - Use it as the OpenSearch document `_id` (Fluent Bit `Id_Key`), so duplicates become idempotent overwrites.
  - Optionally use the Kafka idempotent producer, or produce the whole batch and then await all delivery futures.

### ~~C2. Non-2xx "specified" responses drop the in-memory batch~~ (Status-code classification & 401 token invalidation still open) — 🟠 `reliability`
- ~~In [`_sendAll`](../mobile/remote_logging/lib/src/remote/log_sender_service.dart), the `SpecifiedResponseResult` branch logs an error but **doesn't** call `_addFailedBatch`. The batch only survives in SQLite (if enabled) until the next launch.~~ *(Implemented: `_sendAll` now calls `_addFailedBatch(request)` on `SpecifiedResponseResult`)*
- Neither `_sendAll` nor `_resendFailedRequests` classifies status codes or invalidates `_authService` on `401 Unauthorized`.
- **Suggestions:** classify responses:
  - `4xx` other than 401/429 → drop and remove locally (poison data)
  - `401` → call `_authService.invalidate()`, re-auth, and retry
  - `429` / `5xx` → retry with backoff

### C3. Path channel is not bound to the token — 🟡 `security`
- [`request_filter`](../logger_gateway/src/proxy/handler.rs) parses `{channel}` from `/v1/logs/{channel}` but doesn't check it against the JWT. The JWT has no channel claim, so a token from channel A is accepted on `/v1/logs/B`.
- Data still lands in the token's `app` index (via `x-app-id`), so impact is low. ~~But the OpenAPI description says the channel is enforced, which is misleading.~~ *(OpenAPI description fixed in `openapi.json`)*
- **Suggestion:** add a `ch` claim at exchange and enforce `ch == path channel` in the gateway.

### C4. JWT signing key has no `kid` and no rotation support — 🟡 `ops`
- Tokens are minted with `Header::new(ES256)` and no `kid`, and the gateway trusts one public key. Rotating the key invalidates every live 24h token at once.
- **Suggestion:** add `kid`, let the gateway accept a key set (current + previous), and rotate with overlap ≥ access TTL.

### C5. Error JSON built by string formatting — 🟡 `reliability`
- `send_error` in the gateway escapes only `"`. Messages containing `\` or control characters (e.g. JWT error text) can produce invalid JSON.
- **Suggestion:** use `serde_json::json!`.

### ~~C6. Stale docs and tooling~~ — 🟡 `dx`
- ~~[`validation_worker/README.md`](../validation_worker/README.md) still says the validation worker mints JWTs and lists `REFRESH_JWT_TTL_SECONDS`.~~ *(Implemented: cleaned up in `validation_worker/README.md`, `docker-compose.yaml`, `kubernetes/auth-worker/auth-worker.yaml`, and `validation_worker/config/channels.yaml`)*
- [`load_tests/stress_test.js`](../load_tests/stress_test.js) hard-codes a 15-minute token assumption.

### C7. Integration tests need Docker — 🟡 `dx`
- `validation_worker/tests/integration_test.rs` fails without a Docker socket (testcontainers), though `SKIP_INTEGRATION=1` opt-out is implemented in [`integration_test.rs`](../validation_worker/tests/integration_test.rs).
- **Suggestion:** gate it behind a feature flag or `#[ignore]` plus a CI job that provides Docker, so bare `cargo test` stays green locally without setting `SKIP_INTEGRATION=1`.

### ~~C8. Re-enrollment recovery when local hardware key is lost (`prove()` failure)~~ — 🟠 `reliability`
- *(Implemented in [`AuthService._exchangeWithAttestation`](../mobile/remote_logging/lib/src/remote/auth_service.dart): `attestationProvider.prove(...)` is wrapped in `try / catch`; when `!isRetry`, it calls `await attestationProvider.reset()` and retries enrollment once.)*

### ~~C9. Avoid wiping hardware key on `5xx` server errors during `/auth/exchange`~~ — 🟠 `reliability`
- *(Implemented in [`AuthService._exchangeWithAttestation`](../mobile/remote_logging/lib/src/remote/auth_service.dart): `SpecifiedResponseResult` only resets the hardware key and re-enrolls on HTTP `400`, `401`, or `403`, preserving the 90-day hardware key across transient `5xx` outages.)*

---

## D. Product suggestions (not planned)

| Suggestion | Value |
|---|---|
| Dashboard panel: share of sessions by `att` level per app version | Data for deciding when to flip `monitor` → `enforce` |
| Report attestation failure reasons (`unsupported`, `entitlement_missing`, `revoked`, `boot_unverified`) as a metric | Fast diagnosis of rollout problems |
| Per-channel kill switch to set all sessions to `att=none` | Emergency bypass if Apple/Google roots or the revocation list break |
| Log sampling for `att=none` sessions in `monitor` mode | Cost control without losing visibility |
