//! Tier-2 Integration Tests — ephemeral Kafka via testcontainers.
//!
//! Each test spins up a fresh Kafka container (pulled once, cached by Docker),
//! builds the Axum app pointing at it, sends real HTTP requests, and asserts
//! both the HTTP response and the Kafka message payload.
//!
//! # Requirements
//! Docker must be running on the host machine.
//!
//! # Opting out
//! ```text
//! SKIP_INTEGRATION=1 cargo test --test integration_test
//! ```

use futures::StreamExt;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    ClientConfig, Message,
};
use reqwest::StatusCode;
use std::{sync::Arc, time::Duration};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::kafka::Kafka;
use tokio::net::TcpListener;
use validation_worker::{
    auth::attestation::NoopVerifier,
    build_app,
    config::WorkerConfig,
    AppState, RdkafkaSink,
};
use arc_swap::ArcSwap;

const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

// ── Helper ────────────────────────────────────────────────────────────────────

/// The external client port exposed by the Confluent Kafka testcontainer module.
///
/// The container maps two listeners:
/// - `BROKER://0.0.0.0:9092` — internal inter-broker (not for external clients)
/// - `PLAINTEXT://0.0.0.0:9093` — external client listener (KAFKA_PORT)
///
/// `exec_after_start` inside the module patches `advertised.listeners` to
/// `PLAINTEXT://127.0.0.1:<mapped-9093>` so rdkafka can reach the broker
/// from the host after the initial metadata exchange.
const KAFKA_CONTAINER_PORT: u16 = 9093;

/// Poll a TCP port until it accepts a connection or `timeout` elapses.
///
/// Used to wait for both the Kafka container and the Axum server to be ready
/// before sending requests, preventing flaky "connection refused" failures.
async fn wait_for_port(host: &str, port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if tokio::net::TcpStream::connect((host, port)).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// Build a bootstrap servers string from the started Kafka container.
///
/// Uses `KAFKA_CONTAINER_PORT` (9093) — the PLAINTEXT external listener whose
/// advertised address is patched by the module's `exec_after_start` hook.
/// Using port 9092 (the BROKER listener) would return the un-patched internal
/// address and cause rdkafka to fail after the first metadata fetch.
async fn kafka_bootstrap(container: &testcontainers::ContainerAsync<Kafka>) -> String {
    let port = container
        .get_host_port_ipv4(KAFKA_CONTAINER_PORT)
        .await
        .expect("Failed to get Kafka mapped port");
    format!("127.0.0.1:{}", port)
}

// ── Test 1: Happy path — valid envelope → 200 + Kafka message verified ────────

/// Full round-trip test:
/// 1. Start ephemeral Kafka container.
/// 2. POST a valid `IngestEnvelope` to the Axum worker.
/// 3. Assert 200 OK.
/// 4. Consume the Kafka message and verify the flattened `KafkaPayload` structure
///    matches the spec (spec report §4 — no nested `payload` key).
#[tokio::test]
async fn integration_valid_payload_returns_200_and_reaches_kafka() {
    if std::env::var("SKIP_INTEGRATION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        eprintln!("Skipping integration test: SKIP_INTEGRATION is set");
        return;
    }

    // ── 1. Start Kafka container ──────────────────────────────────────────────
    let kafka_container = Kafka::default()
        .start()
        .await
        .expect("Failed to start Kafka testcontainer — is Docker running?");

    let bootstrap = kafka_bootstrap(&kafka_container).await;
    eprintln!("[integration] Kafka bootstrap: {bootstrap}");

    // Wait for the external listener port (9093) to be reachable.
    let port: u16 = bootstrap.split(':').last().unwrap().parse().unwrap();
    assert!(
        wait_for_port("127.0.0.1", port, Duration::from_secs(30)).await,
        "Kafka did not become connectable within 30 s"
    );

    // Additional grace period for exec_after_start (kafka-configs --alter) to
    // finish patching advertised.listeners and for leader election to complete.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // ── 2. Build Axum app pointing at the container ───────────────────────────
    let sink = RdkafkaSink::new(&bootstrap).expect("Failed to create RdkafkaSink");
    let state = Arc::new(AppState {
        sink: Arc::new(sink),
        kafka_topic: "mobile-logs".to_string(),
        metadata_topic: "device-metadata".to_string(),
        device_index_prefix: "mobile-devices".to_string(),
        verifier: Arc::new(NoopVerifier),
        worker_config: Arc::new(ArcSwap::from_pointee(WorkerConfig { channels: vec![] })),
        jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
        access_ttl_seconds: 900,
        refresh_ttl_seconds: 604800,
        jwt_issuer: "test".to_string(),
    });
    let app = build_app(state);

    // Bind to OS-assigned free port to avoid conflicts with any running services.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let server_port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Axum server error");
    });

    // Wait for the Axum server to start accepting connections.
    assert!(
        wait_for_port("127.0.0.1", server_port, Duration::from_secs(5)).await,
        "Axum server did not start in time"
    );

    // ── 3. POST a valid IngestEnvelope ────────────────────────────────────────
    //
    // Note: `sessionId` is intentionally omitted from the inner payload to
    // simulate the real Dart client behaviour (spec report §3).
    let client = reqwest::Client::new();
    let envelope = serde_json::json!({
        "timestamp":  "2026-06-23T15:00:00Z",
        "loggerName": "IntegrationTestLogger",
        "message":    "Integration test log entry.",
        "level":      "INFO",
        "zone":       "main",
        "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d"
    });

    let response = client
        .post(format!(
            "http://127.0.0.1:{}/v1/logs/test-channel",
            server_port
        ))
        .header("x-app-id", "com.test.app")
        .json(&envelope)
        .send()
        .await
        .expect("HTTP request to Axum server failed");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK from ingest endpoint, got {:?}",
        response.status()
    );

    // ── 4. Consume and verify the Kafka message ───────────────────────────────
    //
    // We create a fresh consumer *after* the message was produced so that
    // `auto.offset.reset = earliest` fetches the message we just sent.
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &bootstrap)
        .set("group.id", "integration-test-consumer-1")
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("Failed to create StreamConsumer");

    consumer
        .subscribe(&["mobile-logs"])
        .expect("Failed to subscribe to mobile-logs topic");

    let msg = tokio::time::timeout(
        Duration::from_secs(15),
        consumer.stream().next(),
    )
    .await
    .expect("Timed out waiting for Kafka message (15 s)")
    .expect("Consumer stream ended unexpectedly")
    .expect("Kafka error while consuming message");

    let raw_bytes = msg.payload().expect("Kafka message had no payload");
    let kafka_json: serde_json::Value =
        serde_json::from_slice(raw_bytes).expect("Kafka payload is not valid JSON");

    eprintln!("[integration] Kafka message: {kafka_json:#}");

    // ── 5. Assert KafkaPayload structure (spec report §4) ────────────────────

    // Top-level enrichment fields added by the worker.
    assert_eq!(kafka_json["app_id"],            "com.test.app",           "app_id");
    assert_eq!(kafka_json["log_index_prefix"], "mobile-logs-com.test.app","log_index_prefix");

    // ServerLog fields are FLATTENED into the root — verify camelCase renames.
    assert_eq!(
        kafka_json["deviceUUID"],
        "a809b022-7934-4b53-90d2-bf7cb95a947d",
        "deviceUUID must be flattened to root level"
    );
    assert_eq!(kafka_json["loggerName"], "IntegrationTestLogger", "loggerName");
    assert_eq!(kafka_json["level"],      "INFO",                  "level");
    assert_eq!(kafka_json["message"],    "Integration test log entry.", "message");

    // CRITICAL (spec report §4): there must be NO nested "payload" wrapper key.
    assert!(
        kafka_json.get("payload").is_none(),
        "KafkaPayload MUST NOT contain a nested 'payload' key — ServerLog \
         must be flattened (spec report §4). Got: {kafka_json:#}"
    );

    // Spec report §3: sessionId was not sent by the client.
    // It should be absent from the JSON or serialised as null.
    let session_id = kafka_json.get("sessionId");
    assert!(
        session_id.is_none() || session_id.unwrap().is_null(),
        "sessionId should be absent or null when not sent by the mobile client \
         (spec report §3). Got: {session_id:?}"
    );
}

// ── Test 2: Validation error paths — real HTTP server, no Kafka consumption ───

/// Verifies that validation failures return 400 before the handler ever touches
/// the Kafka sink. Runs against a real Axum server (backed by a real broker
/// for completeness) so the test path is identical to production.
#[tokio::test]
async fn integration_invalid_payloads_return_400() {
    if std::env::var("SKIP_INTEGRATION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        eprintln!("Skipping integration test: SKIP_INTEGRATION is set");
        return;
    }

    // Start Kafka (needed for the app to boot cleanly).
    let kafka_container = Kafka::default()
        .start()
        .await
        .expect("Failed to start Kafka testcontainer");

    let bootstrap = kafka_bootstrap(&kafka_container).await;
    let port: u16 = bootstrap.split(':').last().unwrap().parse().unwrap();

    assert!(
        wait_for_port("127.0.0.1", port, Duration::from_secs(30)).await,
        "Kafka did not become connectable"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;

    let sink = RdkafkaSink::new(&bootstrap).unwrap();
    let state = Arc::new(AppState {
        sink: Arc::new(sink),
        kafka_topic: "mobile-logs".to_string(),
        metadata_topic: "device-metadata".to_string(),
        device_index_prefix: "mobile-devices".to_string(),
        verifier: Arc::new(NoopVerifier),
        worker_config: Arc::new(ArcSwap::from_pointee(WorkerConfig { channels: vec![] })),
        jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
        access_ttl_seconds: 900,
        refresh_ttl_seconds: 604800,
        jwt_issuer: "test".to_string(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, build_app(state)).await.unwrap();
    });
    assert!(wait_for_port("127.0.0.1", server_port, Duration::from_secs(5)).await);

    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}/v1/logs/ch", server_port);

    // Case A — Spec report §2: raw `ServerLog` body missing required fields (empty object)
    let raw_log = serde_json::json!({});
    let resp = client.post(&base_url)
        .header("x-app-id", "com.test.app")
        .json(&raw_log).send().await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "Case A: Empty payload must be rejected"
    );

    // Case B — Spec report §2: valid payload but `deviceUUID` missing
    let missing_uuid = serde_json::json!({
        "timestamp":  "2026-06-23T15:00:00Z",
        "loggerName": "TestLogger",
        "message":    "missing uuid",
        "level":      "DEBUG",
        "zone":       ""
        // "deviceUUID" intentionally omitted
    });
    let resp = client
        .post(&base_url)
        .header("x-app-id", "com.test.app")
        .json(&missing_uuid)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "Case B: Missing deviceUUID must be rejected"
    );

    // Case C — Empty identifiers in headers
    let empty_ids = serde_json::json!({
        "timestamp":  "2026-06-23T15:00:00Z",
        "loggerName": "TestLogger",
        "message":    "no identity",
        "level":      "DEBUG",
        "zone":       "",
        "deviceUUID": "some-uuid"
    });
    let resp = client.post(&base_url)
        .header("x-app-id", "")
        .json(&empty_ids).send().await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "Case C: Empty headers must be rejected with 401"
    );
}
