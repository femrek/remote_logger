use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

// ── Async plumbing ────────────────────────────────────────────────────────────

/// Object-safe boxed future used as the return type of [`KafkaSink::send`].
/// Avoids pulling in `async-trait` while remaining fully compatible with
/// `Arc<dyn KafkaSink>` trait objects.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Delivery error returned by a [`KafkaSink`] implementation.
#[derive(Debug)]
pub struct KafkaSinkError(pub String);

impl std::fmt::Display for KafkaSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KafkaSinkError: {}", self.0)
    }
}

// ── KafkaSink trait ───────────────────────────────────────────────────────────

/// An object-safe, async abstraction over a Kafka message producer.
///
/// Decoupling the handler from a concrete `FutureProducer` lets unit tests
/// inject a zero-network [`NoopSink`] while production code uses the real
/// [`RdkafkaSink`] backed by `librdkafka`.
pub trait KafkaSink: Send + Sync {
    fn send<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> BoxFuture<'a, Result<(), KafkaSinkError>>;
}

// ── Production Kafka sink ─────────────────────────────────────────────────────

/// Production [`KafkaSink`] implementation backed by an rdkafka
/// [`FutureProducer`]. Each `send` awaits the broker ACK before resolving,
/// providing a zero-data-loss delivery guarantee.
pub struct RdkafkaSink {
    producer: FutureProducer,
}

impl RdkafkaSink {
    /// Create a new sink connected to the given comma-separated `brokers` list.
    pub fn new(brokers: &str) -> Result<Self, rdkafka::error::KafkaError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("reconnect.backoff.ms", "10000")
            .set("reconnect.backoff.max.ms", "10000")
            .create()?;
        Ok(Self { producer })
    }
}

impl KafkaSink for RdkafkaSink {
    fn send<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> BoxFuture<'a, Result<(), KafkaSinkError>> {
        Box::pin(async move {
            let record = FutureRecord::to(topic).key(key).payload(payload);
            self.producer
                .send(record, Duration::from_secs(5))
                .await
                .map(|_| ())
                .map_err(|(e, _)| KafkaSinkError(e.to_string()))
        })
    }
}

// ── Application state ─────────────────────────────────────────────────────────

/// Shared application state injected into every handler via `State<Arc<AppState>>`.
///
/// `sink` is a trait object so that tests can substitute a no-op implementation
/// without touching any handler code.
pub struct AppState {
    pub sink: Arc<dyn KafkaSink>,
    pub kafka_topic: String,
}

// ── Domain types ──────────────────────────────────────────────────────────────

/// Single log record produced by the mobile client.
///
/// Serde renames match the camelCase JSON keys the Dart client emits.
/// Per spec report §3, `session_id` is `Option<String>` because the Dart
/// client's `toJsonPayload()` never serialises the field.
#[derive(Debug, Deserialize, Serialize)]
pub struct ServerLog {
    pub timestamp: String,
    #[serde(rename = "loggerName")]
    pub logger_name: String,
    pub message: String,
    pub level: String,
    pub zone: String,
    /// Made `Option` intentionally — the mobile client omits this field.
    /// See spec report §3: "Missing sessionId Serialization on the Client".
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    /// Required by the OpenAPI spec and present in every mobile payload.
    #[serde(rename = "deviceUUID")]
    pub device_uuid: String,
    pub error: Option<String>,
    pub stacktrace: Option<String>,
}

/// Wrapper that the Pingora gateway injects before forwarding to this worker.
///
/// Per spec report §2: sending a raw `ServerLog` without this envelope will
/// cause serde to reject the body and return 400 Bad Request.
#[derive(Debug, Deserialize)]
pub struct IngestEnvelope {
    pub app_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
}

/// Final message serialised to Kafka.
///
/// `#[serde(flatten)]` expands `ServerLog` fields directly into the root
/// object — there is **no** nested `payload` key in the Kafka message.
/// Per spec report §4, Fluent Bit's `nest/lift` filter is therefore a no-op.
#[derive(Debug, Serialize, Deserialize)]
pub struct KafkaPayload {
    pub tenant_id: String,
    pub app_id: String,
    pub log_index_prefix: String,
    #[serde(flatten)]
    pub log: ServerLog,
}

// ── Error type ────────────────────────────────────────────────────────────────

pub enum AppError {
    BadRequest,
    ServiceUnavailable,
    InternalError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest => (StatusCode::BAD_REQUEST, "Bad Request"),
            AppError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable"),
            AppError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };
        (status, msg).into_response()
    }
}

// ── Request handler ───────────────────────────────────────────────────────────

/// `POST /ingest` and `POST /v1/logs/:channel` handler.
///
/// Processing pipeline:
/// 1. Extract `x-tenant-id` / `x-app-id` from request headers.
/// 2. Parse the `IngestEnvelope` wrapper from the JSON body; 400 on failure.
/// 3. Fall back to envelope fields if headers are absent.
/// 4. Validate the inner `payload` against the strict `ServerLog` schema; 400 on failure.
/// 5. Serialise `KafkaPayload` and await broker ACK via the injected sink; 503 on failure.
pub async fn ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<IngestEnvelope>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    // ── 1. Header extraction ──────────────────────────────────────────────────
    let mut tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let mut app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    tracing::info!(
        tenant_id = %tenant_id,
        app_id = %app_id,
        "Received ingest request"
    );

    // ── 2. Parse envelope — reject bare ServerLog bodies (spec report §2) ────
    let Json(envelope) = match payload {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse IngestEnvelope JSON body");
            return Err(AppError::BadRequest);
        }
    };

    // ── 3. Fall back to envelope fields when headers are absent ──────────────
    if tenant_id.is_empty() {
        tenant_id = envelope.tenant_id.clone();
    }
    if app_id.is_empty() {
        app_id = envelope.app_id.clone();
    }

    if tenant_id.is_empty() || app_id.is_empty() {
        tracing::warn!(
            tenant_id = %tenant_id,
            app_id = %app_id,
            "Rejected: missing tenant/app identifiers in both headers and envelope"
        );
        return Err(AppError::BadRequest);
    }

    // ── 4. Deep-validate inner payload against strict ServerLog schema ────────
    let log_payload: ServerLog = match serde_json::from_value(envelope.payload) {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                app_id = %app_id,
                error = %err,
                "Failed to validate inner payload against ServerLog schema"
            );
            return Err(AppError::BadRequest);
        }
    };

    // ── 5. Assemble the flattened Kafka message ───────────────────────────────
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
            return Err(AppError::InternalError);
        }
    };

    // ── 6. Zero-data-loss delivery: await broker ACK ─────────────────────────
    match state.sink.send(&state.kafka_topic, &tenant_id, &serialised).await {
        Ok(()) => {
            tracing::info!(tenant_id = %tenant_id, "Message delivered to Kafka");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            tracing::error!(tenant_id = %tenant_id, error = %e, "Kafka delivery failed");
            Err(AppError::ServiceUnavailable)
        }
    }
}

// ── App builder ───────────────────────────────────────────────────────────────

/// Build the Axum [`Router`] wired to `state`.
///
/// Exposed as `pub` so both the binary entry-point and integration-test harnesses
/// share the exact same routing configuration — no duplication, no divergence.
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // Original ingest alias kept for backwards-compatibility.
        .route("/ingest", post(ingest_handler))
        // Workaround route — see spec report §1 (routing mismatch).
        // Clients MUST post to /v1/logs/{channel} without the /log suffix.
        .route("/v1/logs/:channel", post(ingest_handler))
        .with_state(state)
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
    use super::*;
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt; // for Router::oneshot

    // ── No-op sink ────────────────────────────────────────────────────────────

    /// In-process [`KafkaSink`] that immediately returns `Ok(())`.
    ///
    /// This lets unit tests exercise the handler's full validation pipeline up
    /// to and including the Kafka dispatch step, without requiring a real broker.
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

    fn test_app() -> Router {
        let state = Arc::new(AppState {
            sink: Arc::new(NoopSink),
            kafka_topic: "test-topic".to_string(),
        });
        build_app(state)
    }

    /// Canonical valid `IngestEnvelope` mirroring what the Dart client sends.
    /// `sessionId` is deliberately absent — see spec report §3.
    fn valid_envelope() -> serde_json::Value {
        serde_json::json!({
            "app_id":    "dev.femrek.exampleapp.dev",
            "tenant_id": "example_tenant",
            "payload": {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "AppDatabase",
                "message":    "Connection established.",
                "level":      "INFO",
                "zone":       "",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
            }
        })
    }

    async fn post_json(app: Router, uri: &str, body: serde_json::Value) -> http::Response<Body> {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn post_json_with_headers(
        app: Router,
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

    // ── Test cases ────────────────────────────────────────────────────────────

    /// Happy path: a fully-formed `IngestEnvelope` with all required `ServerLog`
    /// fields (and `sessionId` absent, matching real Dart client behaviour)
    /// must be accepted and forwarded to the sink → 200 OK.
    #[tokio::test]
    async fn test_valid_envelope_returns_200() {
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", valid_envelope()).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// The `/ingest` alias route must behave identically to `/v1/logs/:channel`.
    #[tokio::test]
    async fn test_ingest_alias_route_returns_200() {
        let resp = post_json(test_app(), "/ingest", valid_envelope()).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Spec report §2 — Sending a raw `ServerLog` directly (no `IngestEnvelope`
    /// wrapper) must be rejected. The serde deserialiser requires the `app_id`,
    /// `tenant_id`, and `payload` keys to be present at the top level.
    #[tokio::test]
    async fn test_raw_server_log_without_envelope_returns_400() {
        let raw_server_log = serde_json::json!({
            "timestamp":  "2026-06-23T15:00:00Z",
            "loggerName": "AppDatabase",
            "message":    "Connection established.",
            "level":      "INFO",
            "zone":       "",
            "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
        });
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", raw_server_log).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "A raw ServerLog body must be rejected (spec report §2)"
        );
    }

    /// Spec report §2 — The inner `payload` is deep-validated against the strict
    /// `ServerLog` schema. A payload missing the required `deviceUUID` field must
    /// fail that validation and return 400.
    #[tokio::test]
    async fn test_missing_device_uuid_in_payload_returns_400() {
        let envelope = serde_json::json!({
            "app_id":    "dev.femrek.exampleapp.dev",
            "tenant_id": "example_tenant",
            "payload": {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "AppDatabase",
                "message":    "Connection established.",
                "level":      "INFO",
                "zone":       ""
                // "deviceUUID" intentionally omitted
            }
        });
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", envelope).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "A payload missing deviceUUID must be rejected (spec report §2)"
        );
    }

    /// When both `x-tenant-id`/`x-app-id` headers are absent AND the envelope
    /// carries empty strings for both fields, the handler cannot determine
    /// tenancy and must reject the request with 400.
    #[tokio::test]
    async fn test_empty_identifiers_no_headers_returns_400() {
        let envelope = serde_json::json!({
            "app_id":    "",
            "tenant_id": "",
            "payload": {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "AppDatabase",
                "message":    "Connection established.",
                "level":      "INFO",
                "zone":       "",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
            }
        });
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", envelope).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "Empty identifiers in both headers and envelope must be rejected"
        );
    }

    /// Spec report §3 — The mobile client never serialises `sessionId`. Because
    /// the Rust struct defines it as `Option<String>`, its absence must not cause
    /// a validation failure. This test explicitly documents that contract.
    #[tokio::test]
    async fn test_session_id_is_optional_and_omission_returns_200() {
        // `valid_envelope()` already omits sessionId — this test makes the intent explicit.
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", valid_envelope()).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "Omitting sessionId must not fail validation (spec report §3)"
        );
    }

    /// When `x-tenant-id` and `x-app-id` headers carry the identity, the handler
    /// must override the empty envelope fields. The request must succeed.
    #[tokio::test]
    async fn test_header_ids_accepted_when_envelope_ids_empty() {
        let envelope = serde_json::json!({
            "app_id":    "",    // empty — will be overridden by header
            "tenant_id": "",   // empty — will be overridden by header
            "payload": {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "AppDatabase",
                "message":    "Connection established.",
                "level":      "INFO",
                "zone":       "",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
            }
        });
        let resp = post_json_with_headers(
            test_app(),
            "/v1/logs/example-app-dev",
            envelope,
            &[
                ("x-tenant-id", "example_tenant"),
                ("x-app-id", "dev.femrek.exampleapp.dev"),
            ],
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "Header identifiers must override empty envelope fields"
        );
    }

    /// Optional `error` and `stacktrace` fields are accepted transparently when present.
    #[tokio::test]
    async fn test_optional_error_and_stacktrace_fields_accepted() {
        let envelope = serde_json::json!({
            "app_id":    "dev.femrek.exampleapp.dev",
            "tenant_id": "example_tenant",
            "payload": {
                "timestamp":  "2026-06-23T15:00:00Z",
                "loggerName": "CrashReporter",
                "message":    "Unhandled exception.",
                "level":      "SEVERE",
                "zone":       "",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
                "error":      "NullPointerException at Foo.bar:12",
                "stacktrace": "#0  Foo.bar (package:app/foo.dart:12)"
            }
        });
        let resp = post_json(test_app(), "/v1/logs/example-app-dev", envelope).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Spec report §1 — the worker's Axum router has no wildcard sub-routes.
    /// Any request path ending in `/log`, `/log/batch`, or `/log/device` falls
    /// through to Axum's built-in 404 handler. This test documents the
    /// *known* routing limitation so future changes don't silently break it.
    #[tokio::test]
    async fn test_path_with_log_suffix_returns_404() {
        let resp = post_json(
            test_app(),
            "/v1/logs/example-app-dev/log",
            valid_envelope(),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "Sub-paths (/log, /log/batch, /log/device) are not routed (spec report §1)"
        );
    }
}
