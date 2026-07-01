//! `POST /ingest` and `POST /v1/logs/:channel` handler.
//!
//! ## Request contract (post-refactor)
//!
//! The gateway validates the client's JWT and injects two mandatory headers:
//! - `x-tenant-id` — tenant identifier from the JWT `tid` claim.
//! - `x-app-id`   — application ID from the JWT `app` claim.
//!
//! The HTTP body MUST be a raw `ServerLog` JSON object. The `IngestEnvelope`
//! wrapper (`{ app_id, tenant_id, payload }`) has been removed; the gateway
//! now carries identity exclusively via headers.
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
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing x-tenant-id header — request did not pass through gateway JWT validation");
            AppError::Unauthorized
        })?
        .to_string();

    let app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing x-app-id header — request did not pass through gateway JWT validation");
            AppError::Unauthorized
        })?
        .to_string();

    tracing::info!(
        tenant_id = %tenant_id,
        app_id = %app_id,
        "Received ingest request"
    );

    // ── 2. Parse raw ServerLog body ───────────────────────────────────────────
    let Json(log_payload) = match payload {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse ServerLog JSON body");
            let elapsed = start.elapsed().as_secs_f64();
            REQUESTS_TOTAL.with_label_values(&[&app_id, "400"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            return Err(AppError::BadRequest(format!("Invalid ServerLog body: {err}")));
        }
    };

    // ── 3. Assemble the flattened Kafka message ───────────────────────────────
    let kafka_payload = KafkaPayload {
        tenant_id: tenant_id.clone(),
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
    match state.sink.send(&state.kafka_topic, &tenant_id, &serialised).await {
        Ok(()) => {
            tracing::info!(tenant_id = %tenant_id, "Message delivered to Kafka");
            REQUESTS_TOTAL.with_label_values(&[&app_id, "200"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            Ok(axum::http::StatusCode::OK)
        }
        Err(e) => {
            tracing::error!(tenant_id = %tenant_id, error = %e, "Kafka delivery failed");
            REQUESTS_TOTAL.with_label_values(&[&app_id, "503"]).inc();
            REQUEST_DURATION_SECONDS.with_label_values(&[&app_id]).observe(elapsed);
            Err(AppError::ServiceUnavailable)
        }
    }
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
        auth::attestation::{NoopVerifier, PlatformDispatchVerifier},
        config::{WorkerChannelConfig, WorkerConfig},
        kafka::{BoxFuture, KafkaSink, KafkaSinkError},
    };
    use arc_swap::ArcSwap;

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
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig {
            channels: vec![WorkerChannelConfig {
                id: "example-app-dev".to_string(),
                app_id: "dev.femrek.exampleapp.dev".to_string(),
                validate_attestation: false,
                google_project_number: None,
                apple_team_id: None,
                require_apple_key_hash: false,
            }],
        }));
        let state = Arc::new(AppState {
            sink: Arc::new(NoopSink),
            kafka_topic: "test-topic".to_string(),
            metadata_topic: "device-metadata".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(NoopVerifier),
                ios: Box::new(NoopVerifier),
            }),
            worker_config,
            jwt_secret: "test-secret".to_string(),
            access_ttl_seconds: 900,
            refresh_ttl_seconds: 604_800,
            jwt_issuer: "logger-gateway".to_string(),
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
            "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
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
                ("x-tenant-id", "example_tenant"),
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
                ("x-tenant-id", "example_tenant"),
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Missing `x-tenant-id` header → 401 (gateway authentication bypassed).
    #[tokio::test]
    async fn test_missing_tenant_id_header_returns_401() {
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            valid_server_log(),
            &[("x-app-id", "dev.femrek.exampleapp.dev")],
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "Missing x-tenant-id must be rejected with 401"
        );
    }

    /// Missing `x-app-id` header → 401.
    #[tokio::test]
    async fn test_missing_app_id_header_returns_401() {
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            valid_server_log(),
            &[("x-tenant-id", "example_tenant")],
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "Missing x-app-id must be rejected with 401"
        );
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
                    .header("x-tenant-id", "example_tenant")
                    .header("x-app-id", "dev.femrek.exampleapp.dev")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// Missing required `deviceUUID` field → 400 (serde rejects the body).
    #[tokio::test]
    async fn test_missing_device_uuid_returns_400() {
        let body = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Connection established.",
            "level":      "INFO",
            "zone":       ""
            // "deviceUUID" intentionally omitted
        });
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-tenant-id", "example_tenant"),
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
                ("x-tenant-id", "example_tenant"),
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
            "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "error":      "NullPointerException at Foo.bar:12",
            "stacktrace": "#0  Foo.bar (package:app/foo.dart:12)"
        });
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            body,
            &[
                ("x-tenant-id", "example_tenant"),
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
                ("x-tenant-id", "example_tenant"),
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
