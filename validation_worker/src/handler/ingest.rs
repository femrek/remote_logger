//! `POST /ingest` and `POST /v1/logs/:channel` handler.
//!
//! ## Request contract (post-refactor)
//!
//! The gateway validates the client's JWT and injects internal headers:
//! - `x-app-id`      — application ID from the JWT `app` claim (mandatory).
//! - `x-device-id`   — device ID from the JWT `sub` claim.
//! - `x-session-id`  — session ID from the JWT `session_id` claim.
//! - `x-attestation` — hardware attestation tier from the JWT `att` claim.
//!
//! The HTTP body MUST be a raw `ServerLog` JSON object from the client.
//! The validation worker does NOT expect `*IdJwt` fields from the client;
//! it reads `deviceIdJwt` and `sessionIdJwt` strictly from the gateway headers
//! and stamps them onto the Kafka payload.
//!
//! ## Processing pipeline
//!
//! 1. Extract `x-tenant-id` / `x-app-id` from request headers — 401 if absent.
//! 2. Parse the raw `ServerLog` from the JSON body — 400 on failure.
//! 3. Validate the `ServerLog` fields (handled implicitly by serde).
//! 4. Serialise `KafkaPayload` and await broker ACK — 503 on failure.

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::HeaderMap,
    response::IntoResponse,
};

use crate::{
    app::{AppError, AppState},
    domain::{KafkaPayload, ServerLog},
    metrics::{REQUEST_DURATION_SECONDS, REQUESTS_TOTAL},
};
use std::time::Instant;

// ── Request handler ───────────────────────────────────────────────────────────

/// `POST /ingest` and `POST /v1/logs/:channel` handler.
pub async fn ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<ServerLog>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let start = Instant::now();

    // ── 1. Header extraction — REQUIRED (gateway always injects these) ─────────
    let app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing x-app-id header — request did not pass through gateway JWT validation");
            AppError::Unauthorized
        })?
        .to_string();

    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let attestation = headers
        .get("x-attestation")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    tracing::info!(
        app_id = %app_id,
        "Received ingest request"
    );

    // ── 2. Parse raw ServerLog body ───────────────────────────────────────────
    let Json(mut log_payload) = match payload {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse ServerLog JSON body");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "400"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::BadRequest(format!("Invalid ServerLog body: {err}")));
        }
    };

    // Stamped from gateway headers (JWT claims)
    log_payload.device_id_jwt = device_id;
    log_payload.session_id_jwt = session_id;
    // Set attestation tier from gateway-verified JWT claims header
    log_payload.attestation = attestation;

    let has_device_id = log_payload
        .device_id
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
        || log_payload
            .device_id_jwt
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

    if !has_device_id {
        tracing::warn!("device_id is missing from both headers and payload");
        let elapsed = start.elapsed().as_secs_f64();
        REQUESTS_TOTAL.with_label_values(&[&app_id, "400"]).inc();
        REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
        return Err(AppError::BadRequest("Missing deviceId".to_string()));
    }

    // ── 3. Assemble the flattened Kafka message ───────────────────────────────
    let kafka_payload = KafkaPayload {
        app_id: app_id.clone(),
        log_index_prefix: format!("mobile-logs-{}", app_id),
        log: log_payload,
    };

    let serialised = match serde_json::to_string(&kafka_payload) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(error = %err, "Failed to serialise KafkaPayload");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "500"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::InternalError);
        }
    };

    // ── 4. Zero-data-loss delivery: await broker ACK ──────────────────────────
    let elapsed = start.elapsed().as_secs_f64();
    match state.sink.send(&state.kafka_topic, &app_id, &serialised).await {
        Ok(()) => {
            tracing::info!(app_id = %app_id, "Message delivered to Kafka");
            REQUESTS_TOTAL.with_label_values(&[&app_id, "200"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            Ok(axum::http::StatusCode::OK)
        }
        Err(e) => {
            tracing::error!(app_id = %app_id, error = %e, "Kafka delivery failed");
            REQUESTS_TOTAL.with_label_values(&[&app_id, "503"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            Err(AppError::ServiceUnavailable)
        }
    }
}

/// `POST /v1/logs/:channel/log/batch` handler.
pub async fn batch_ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<Vec<ServerLog>>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let start = Instant::now();

    // ── 1. Header extraction — REQUIRED (gateway always injects these) ─────────
    let app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing x-app-id header — request did not pass through gateway JWT validation");
            AppError::Unauthorized
        })?
        .to_string();

    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let attestation = headers
        .get("x-attestation")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    tracing::info!(
        app_id = %app_id,
        "Received batch ingest request"
    );

    // ── 2. Parse raw Vec<ServerLog> body ──────────────────────────────────────
    let Json(log_payloads) = match payload {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse batch ServerLog JSON body");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "400"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::BadRequest(format!("Invalid batch ServerLog body: {err}")));
        }
    };

    if log_payloads.is_empty() {
        let elapsed = start.elapsed().as_secs_f64();
        REQUESTS_TOTAL.with_label_values(&[&app_id, "200"]).inc();
        REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
        return Ok(axum::http::StatusCode::OK);
    }

    // ── 3. Assemble and deliver each log ──────────────────────────────────────
    for mut log_payload in log_payloads {
        log_payload.device_id_jwt = device_id.clone();
        log_payload.session_id_jwt = session_id.clone();
        log_payload.attestation = attestation.clone();

        let has_device_id = log_payload
            .device_id
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
            || log_payload
                .device_id_jwt
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);

        if !has_device_id {
            tracing::warn!("device_id is missing from both headers and payload in batch item");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "400"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::BadRequest("Missing deviceId in batch item".to_string()));
        }

        let kafka_payload = KafkaPayload {
            app_id: app_id.clone(),
            log_index_prefix: format!("mobile-logs-{}", app_id),
            log: log_payload,
        };

        let serialised = match serde_json::to_string(&kafka_payload) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(error = %err, "Failed to serialise KafkaPayload in batch");
                let elapsed = start.elapsed().as_secs_f64();
                REQUESTS_TOTAL.with_label_values(&[&app_id, "500"]).inc();
                REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
                return Err(AppError::InternalError);
            }
        };

        if let Err(e) = state.sink.send(&state.kafka_topic, &app_id, &serialised).await {
            tracing::error!(app_id = %app_id, error = %e, "Kafka delivery failed in batch");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "503"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::ServiceUnavailable);
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    REQUESTS_TOTAL.with_label_values(&[&app_id, "200"]).inc();
    REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
    Ok(axum::http::StatusCode::OK)
}


// ─────────────────────────────────────────────────────────────────────────────
// Tier 1: Unit Tests
//
// These tests run entirely in-process with no network or Docker dependency.
// The `NoopSink` immediately ACKs every message, letting tests focus purely on
// the validation and routing logic of `ingest_handler`.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::{
        app::{AppState, build_app},
        kafka::{BoxFuture, KafkaSink, KafkaSinkError},
    };

    // ── No-op sink ────────────────────────────────────────────────────────────

    struct NoopSink;
    impl KafkaSink for NoopSink {
        fn send<'a>(
            &'a self,
            _topic: &'a str,
            _key: &'a str,
            _payload: &'a str,
        ) -> BoxFuture<'a, Result<(), KafkaSinkError>> {
            Box::pin(async { Ok(()) })
        }
    }

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn test_app() -> axum::Router {
        let state = Arc::new(AppState {
            sink: Arc::new(NoopSink),
            kafka_topic: "test-topic".to_string(),
        });
        build_app(state)
    }

    /// Canonical valid raw `ServerLog` — no IngestEnvelope wrapper.
    fn valid_server_log() -> serde_json::Value {
        serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Connection established.",
            "level":      "INFO",
            "zone":       "",
            "deviceId":   "a809b022-7934-4b53-90d2-bf7cb95a947d"
        })
    }

    async fn post_json_with_headers(
        app: axum::Router,
        uri: &str,
        body: serde_json::Value,
        extra_headers: &[(&str, &str)],
    ) -> http::Response<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        for (k, v) in extra_headers {
            builder = builder.header(*k, *v);
        }
        app.oneshot(
            builder
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn post_json(app: axum::Router, uri: &str, body: serde_json::Value) -> http::Response<Body> {
        post_json_with_headers(app, uri, body, &[]).await
    }

    // ── Test cases ────────────────────────────────────────────────────────────

    /// Happy path: raw ServerLog body + required gateway headers → 200 OK.
    #[tokio::test]
    async fn test_valid_server_log_with_headers_returns_200() {
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            valid_server_log(),
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// The `/ingest` alias route must behave identically.
    #[tokio::test]
    async fn test_ingest_alias_route_returns_200() {
        let resp = post_json_with_headers(
            test_app(),
            "/ingest",
            valid_server_log(),
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
    /// Both headers absent → 401.
    #[tokio::test]
    async fn test_no_headers_returns_401() {
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", valid_server_log()).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Malformed JSON body → 400.
    #[tokio::test]
    async fn test_invalid_json_returns_400() {
        let app = test_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/logs/example-app-dev")
                    .header("content-type", "application/json")
                    .header("x-app-id", "dev.femrek.exampleapp.dev")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// Missing required `deviceId` field → 400 (serde rejects the body).
    #[tokio::test]
    async fn test_missing_device_id_returns_400() {
        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Connection established.",
            "level":      "INFO",
            "zone":       ""
            // "deviceId" intentionally omitted
        });
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// Optional `sessionId` field may be absent — spec report §3.
    #[tokio::test]
    async fn test_session_id_is_optional_and_omission_returns_200() {
        // valid_server_log() already omits sessionId — this test makes the intent explicit.
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            valid_server_log(),
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Optional `error` and `stacktrace` fields are accepted transparently.
    #[tokio::test]
    async fn test_optional_error_and_stacktrace_fields_accepted() {
        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "CrashReporter",
            "message":    "Unhandled exception.",
            "level":      "SEVERE",
            "zone":       "",
            "deviceId":   "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "error":      "NullPointerException at Foo.bar:12",
            "stacktrace": "#0  Foo.bar (package:app/foo.dart:12)"
        });
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Sub-paths like /log, /log/batch, /log/device fall through to Axum's 404.
    #[tokio::test]
    async fn test_path_with_log_suffix_returns_404() {
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev/log",
            valid_server_log(),
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_batch_logs_returns_200() {
        let body = serde_json::json!([
            valid_server_log(),
            valid_server_log(),
        ]);
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev/batch",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_empty_batch_logs_returns_200() {
        let body = serde_json::json!([]);
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev/batch",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_invalid_batch_json_returns_400() {
        let app = test_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/logs/example-app-dev/batch")
                    .header("content-type", "application/json")
                    .header("x-app-id", "dev.femrek.exampleapp.dev")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[derive(Default)]
    struct CapturingSink {
        sent: std::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl KafkaSink for CapturingSink {
        fn send<'a>(
            &'a self,
            topic: &'a str,
            key: &'a str,
            payload: &'a str,
        ) -> BoxFuture<'a, Result<(), KafkaSinkError>> {
            self.sent
                .lock()
                .unwrap()
                .push((topic.to_string(), key.to_string(), payload.to_string()));
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn test_ingest_populates_device_id_and_session_id_from_headers() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            kafka_topic: "test-topic".to_string(),
        });
        let app = build_app(state);

        // Payload with no deviceId or sessionId in body
        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Connection established.",
            "level":      "INFO",
            "zone":       ""
        });

        let resp = post_json_with_headers(
            app,
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
                ("x-device-id", "jwt-device-id-999"),
                ("x-session-id", "jwt-session-id-888"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (_, _, payload) = &sent[0];
        let kafka_json: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(kafka_json["deviceIdJwt"], "jwt-device-id-999");
        assert_eq!(kafka_json["sessionIdJwt"], "jwt-session-id-888");
        assert!(kafka_json.get("deviceId").is_none() || kafka_json["deviceId"].is_null());
        assert!(kafka_json.get("sessionId").is_none() || kafka_json["sessionId"].is_null());
    }

    #[tokio::test]
    async fn test_ingest_preserves_client_session_id_and_populates_attestation() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            kafka_topic: "test-topic".to_string(),
        });
        let app = build_app(state);

        // Payload with existing client-side sessionId and deviceId
        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Offline log replayed.",
            "level":      "INFO",
            "zone":       "",
            "deviceId":   "client-device-000",
            "sessionId":  "client-historical-session-111"
        });

        let resp = post_json_with_headers(
            app,
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
                ("x-device-id", "jwt-device-id-999"),
                ("x-session-id", "jwt-session-id-current-222"),
                ("x-attestation", "tee"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (_, _, payload) = &sent[0];
        let kafka_json: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(kafka_json["deviceId"], "client-device-000");
        assert_eq!(kafka_json["deviceIdJwt"], "jwt-device-id-999");
        assert_eq!(kafka_json["sessionId"], "client-historical-session-111");
        assert_eq!(kafka_json["sessionIdJwt"], "jwt-session-id-current-222");
        assert_eq!(kafka_json["attestation"], "tee");
    }

    #[tokio::test]
    async fn test_batch_ingest_preserves_client_session_id_and_populates_attestation() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            kafka_topic: "test-topic".to_string(),
        });
        let app = build_app(state);

        let body = serde_json::json!([
            {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "App1",
                "message":    "Log from older session",
                "level":      "INFO",
                "zone":       "",
                "deviceId":   "client-dev-batch-1",
                "sessionId":  "older-session-aaa"
            },
            {
                "timestamp":  "2026-06-23T15:01:00Z",
                "loggerName": "App2",
                "message":    "Log with no session",
                "level":      "INFO",
                "zone":       ""
            }
        ]);

        let resp = post_json_with_headers(
            app,
            "/v1/logs/example-app-dev/batch",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
                ("x-device-id", "jwt-device-id-999"),
                ("x-session-id", "jwt-session-id-current-bbb"),
                ("x-attestation", "software"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);

        let k1: serde_json::Value = serde_json::from_str(&sent[0].2).unwrap();
        assert_eq!(k1["deviceId"], "client-dev-batch-1");
        assert_eq!(k1["deviceIdJwt"], "jwt-device-id-999");
        assert_eq!(k1["sessionId"], "older-session-aaa");
        assert_eq!(k1["sessionIdJwt"], "jwt-session-id-current-bbb");
        assert_eq!(k1["attestation"], "software");

        let k2: serde_json::Value = serde_json::from_str(&sent[1].2).unwrap();
        assert!(k2.get("deviceId").is_none() || k2["deviceId"].is_null());
        assert_eq!(k2["deviceIdJwt"], "jwt-device-id-999");
        assert!(k2.get("sessionId").is_none() || k2["sessionId"].is_null());
        assert_eq!(k2["sessionIdJwt"], "jwt-session-id-current-bbb");
        assert_eq!(k2["attestation"], "software");
    }

    #[tokio::test]
    async fn test_ingest_ignores_client_provided_jwt_fields() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            kafka_topic: "test-topic".to_string(),
        });
        let app = build_app(state);

        // Client attempts to spoof or send deviceIdJwt / sessionIdJwt in body
        let body = serde_json::json!({
            "timestamp":    "2026-06-23T15:00:00Z",
            "loggerName":   "AppDatabase",
            "message":      "Spoof attempt.",
            "level":        "INFO",
            "zone":         "",
            "deviceId":     "legit-client-device",
            "deviceIdJwt":  "spoofed-device-id",
            "sessionIdJwt": "spoofed-session-id",
            "attestation":  "spoofed-tee"
        });

        let resp = post_json_with_headers(
            app,
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
                ("x-device-id", "header-device-id-verified"),
                ("x-session-id", "header-session-id-verified"),
                ("x-attestation", "software"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (_, _, payload) = &sent[0];
        let kafka_json: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(kafka_json["deviceId"], "legit-client-device");
        // Must read from gateway headers, NOT from client body
        assert_eq!(kafka_json["deviceIdJwt"], "header-device-id-verified");
        assert_eq!(kafka_json["sessionIdJwt"], "header-session-id-verified");
        assert_eq!(kafka_json["attestation"], "software");
    }

    #[tokio::test]
    async fn test_ingest_handles_explicit_null_device_id_and_session_id() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            kafka_topic: "test-topic".to_string(),
        });
        let app = build_app(state);

        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Null fields test.",
            "level":      "INFO",
            "zone":       "",
            "deviceId":   null,
            "sessionId":  null
        });

        let resp = post_json_with_headers(
            app,
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-app-id", "dev.femrek.exampleapp.dev"),
                ("x-device-id", "jwt-device-id-999"),
                ("x-session-id", "jwt-session-id-888"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (_, _, payload) = &sent[0];
        let kafka_json: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert!(kafka_json.get("deviceId").is_none() || kafka_json["deviceId"].is_null());
        assert!(kafka_json.get("sessionId").is_none() || kafka_json["sessionId"].is_null());
        assert_eq!(kafka_json["deviceIdJwt"], "jwt-device-id-999");
        assert_eq!(kafka_json["sessionIdJwt"], "jwt-session-id-888");
    }
}
