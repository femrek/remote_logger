//! Tier-3 End-to-End Test — full Docker Compose pipeline.
//!
//! This test exercises the entire log ingestion pipeline in one shot:
//!
//! ```text
//! [test] ──HTTP POST──► logger_gateway (8080)
//!                              │ JWT-authenticated proxy
//!                              ▼
//!                    validation_worker
//!                              │ rdkafka produce
//!                              ▼
//!                    Kafka broker (mobile-logs topic)
//!                              │ Fluent Bit consumes (Flush=2s)
//!                              ▼
//!                    OpenSearch (9200)  ◄── [test] asserts document exists
//! ```
//!
//! # Running
//! ```bash
//! # Default (Docker must be running):
//! cargo test --test e2e_test --manifest-path e2e/Cargo.toml
//!
//! # Skip without removing:
//! SKIP_E2E=1 cargo test --test e2e_test --manifest-path e2e/Cargo.toml
//! ```
//!
//! # Environment
//! Variables are loaded from `e2e/.env.test` at test start.
//! `GATEWAY_JWT_SECRET` is required; set it in `.env.test` before running.

use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAElCR0ys2PS/qszcku8bm+8JJ8wNLK\n\
IOY+VYNghIkMNmY2GtVnOKApnJ6p+q7oXcBOqnImEgwEb83cDdMNp1vIqg==\n\
-----END PUBLIC KEY-----";

// ─── DockerComposeHandle — RAII stack lifecycle ────────────────────────────────

/// Brings the compose stack up on construction and tears it down (volumes
/// included) when dropped — even if the test panics.
struct DockerComposeHandle {
    compose_files: Vec<String>,
}

impl DockerComposeHandle {
    /// Run `docker compose -f <file1> -f <file2> ... up -d --wait` with the supplied
    /// environment variables injected into the child process.
    ///
    /// `--wait` (Docker Compose v2) blocks until every service with a
    /// `healthcheck` reports healthy, removing the need for custom polling.
    fn up(compose_files: &[String], env_vars: &[(&str, &str)]) -> Self {
        eprintln!("[e2e] docker compose up -d --wait …");
        let mut cmd = Command::new("docker");
        cmd.arg("compose");
        for file in compose_files {
            cmd.args(["-f", file]);
        }
        cmd.args(["up", "--build", "-d", "--wait"]);
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
        let status = cmd
            .status()
            .expect("Failed to invoke 'docker compose up' — is Docker running?");
        assert!(
            status.success(),
            "docker compose up exited with non-zero status: {status}"
        );
        Self {
            compose_files: compose_files.to_vec(),
        }
    }
}

impl Drop for DockerComposeHandle {
    fn drop(&mut self) {
        eprintln!("[e2e] Fetching docker compose logs …");
        let mut logs_cmd = Command::new("docker");
        logs_cmd.arg("compose");
        for file in &self.compose_files {
            logs_cmd.args(["-f", file]);
        }
        logs_cmd.arg("logs");
        if let Ok(mut child) = logs_cmd.spawn() {
            let _ = child.wait();
        }

        eprintln!("[e2e] docker compose down -v (teardown) …");
        let mut cmd = Command::new("docker");
        cmd.arg("compose");
        for file in &self.compose_files {
            cmd.args(["-f", file]);
        }
        cmd.args([
            "down",
            "-v",               // Remove named volumes (clean OpenSearch data).
            "--remove-orphans",
        ]);
        let status = cmd.status();
        match status {
            Ok(s) if s.success() => eprintln!("[e2e] Teardown succeeded"),
            Ok(s) => eprintln!("[e2e] WARNING: docker compose down exited with {s}"),
            Err(e) => eprintln!("[e2e] WARNING: docker compose down failed: {e}"),
        }
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

/// Load env vars from `path` into the current process environment.
/// Missing file is logged but not fatal — caller can provide vars via the shell.
fn load_env_file(path: &str) {
    match dotenvy::from_filename(path) {
        Ok(_) => eprintln!("[e2e] Loaded env from {path}"),
        Err(e) => eprintln!("[e2e] Could not load env file '{path}': {e}"),
    }
}

/// Synchronously poll a TCP port until it accepts connections or until
/// `timeout` elapses. Returns `true` on success.
fn wait_for_port(host: &str, port: u16, timeout: Duration) -> bool {
    let addr = format!("{host}:{port}");
    let start = Instant::now();
    while start.elapsed() < timeout {
        if std::net::TcpStream::connect(&addr).is_ok() {
            eprintln!("[e2e] Port {addr} is ready");
            return true;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    eprintln!("[e2e] Timed out waiting for {addr}");
    false
}

/// Milliseconds since the Unix epoch — used to make the log message unique
/// so that a stale document from a previous failed run is never a false positive.
fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

// ─── E2E test ─────────────────────────────────────────────────────────────────

/// Full pipeline test: gateway → worker → Kafka → Fluent Bit → OpenSearch.
///
/// The test is **enabled by default** and can be disabled by setting
/// `SKIP_E2E=1` (or `SKIP_E2E=true`) in the environment or in `e2e/.env.test`.
#[test]
fn e2e_full_pipeline_log_reaches_opensearch() {
    // ── 0. Load .env.test and check opt-out flag ──────────────────────────────
    let manifest_dir = env!("CARGO_MANIFEST_DIR"); // …/remote_logger_backend/e2e
    let env_file = format!("{manifest_dir}/.env.test");
    load_env_file(&env_file);

    if std::env::var("SKIP_E2E")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        eprintln!("[e2e] SKIP_E2E is set — skipping E2E test");
        return;
    }

    let jwt_private_key = std::env::var("GATEWAY_PRIVATE_KEY")
        .unwrap_or_else(|_| TEST_PRIVATE_KEY_PEM.to_string());
    let jwt_public_key = std::env::var("GATEWAY_PUBLIC_KEY")
        .unwrap_or_else(|_| TEST_PUBLIC_KEY_PEM.to_string());

    let admin_token = std::env::var("GATEWAY_ADMIN_TOKEN")
        .unwrap_or_else(|_| "admin-secret-token".to_string());

    let gateway_url = std::env::var("GATEWAY_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    let opensearch_url = std::env::var("OPENSEARCH_URL")
        .unwrap_or_else(|_| "http://localhost:9200".to_string());

    use base64::Engine as _;
    let opensearch_auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("admin:Ax2ZlVoT4nH2FANX")
    );

    let channel = std::env::var("LOG_CHANNEL")
        .unwrap_or_else(|_| "example-app-dev".to_string());

    // ── 1. Resolve paths ──────────────────────────────────────────────────────
    //
    // CARGO_MANIFEST_DIR for this crate = …/remote_logger_backend/e2e
    // The compose file lives one directory up, under docker/.
    let project_root = Path::new(manifest_dir)
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent directory");

    let docker_dir = project_root.join("docker");
    
    let mut compose_file = docker_dir.join("docker-compose.yaml");
    if !compose_file.exists() {
        compose_file = docker_dir.join("docker-compose.yaml");
    }

    let mut compose_dev_file = docker_dir.join("docker-compose.dev.yaml");
    if !compose_dev_file.exists() {
        compose_dev_file = docker_dir.join("docker-compose.dev.yaml");
    }

    assert!(
        compose_file.exists(),
        "docker-compose.yaml or docker-compose.yaml not found at {:?}",
        docker_dir
    );

    let mut compose_files = vec![compose_file.to_str().unwrap().to_string()];
    if compose_dev_file.exists() {
        compose_files.push(compose_dev_file.to_str().unwrap().to_string());
    }

    let keys_dir = docker_dir.join("keys");
    std::fs::create_dir_all(&keys_dir).expect("Failed to create keys directory");
    std::fs::write(keys_dir.join("private.pem"), &jwt_private_key).expect("Failed to write private.pem");
    std::fs::write(keys_dir.join("public.pem"), &jwt_public_key).expect("Failed to write public.pem");

    // ── 2. Bring up the compose stack ────────────────────────────────────────
    //
    // The `_stack` binding is intentionally held for the lifetime of this test.
    // When it goes out of scope (whether by return, panic, or normal exit) the
    // Drop impl runs `docker compose down -v` for guaranteed cleanup.
    let _stack = DockerComposeHandle::up(
        &compose_files,
        &[
            ("GATEWAY_ADMIN_TOKEN", &admin_token),
            ("OPENSEARCH_ADMIN_PASSWORD", "Ax2ZlVoT4nH2FANX"),
        ],
    );

    // ── 2.5. Seed Consul KV configuration ────────────────────────────────────
    eprintln!("[e2e] Waiting for Consul on port 8500 …");
    assert!(
        wait_for_port("localhost", 8500, Duration::from_secs(120)),
        "Consul did not become reachable within 120 s"
    );

    eprintln!("[e2e] Seeding Consul KV gateway config …");
    let consul_url = "http://localhost:8500";
    let global_config = serde_json::json!({
        "jwt": {
            "public_key_env": "GATEWAY_PUBLIC_KEY",
            "issuer": "logger-gateway",
            "ttl_seconds": 900
        },
        "max_body_bytes": 2097152
    });
    let channel_config = serde_json::json!({
        "id": "example-app-dev",
        "app_id": "dev.femrek.exampleapp.dev",
        "validate_attestation": false,
        "require_apple_key_hash": false
    });

    ureq::put(&format!("{consul_url}/v1/kv/gateway/config/global"))
        .send_json(global_config)
        .expect("Failed to seed global config in Consul");

    ureq::put(&format!("{consul_url}/v1/kv/gateway/config/channels/example-app-dev"))
        .send_json(channel_config)
        .expect("Failed to seed channel config in Consul");

    // ── 3. Wait for dependent services ───────────────────────────────────────
    eprintln!("[e2e] Waiting for gateway on port 8080 …");
    assert!(
        wait_for_port("localhost", 8080, Duration::from_secs(300)),
        "Gateway did not become reachable within 300 s"
    );

    // Extra grace period: the gateway might reject connections while it reads
    // the workers.txt file and the validation worker replicas register.
    std::thread::sleep(Duration::from_secs(5));

    eprintln!("[e2e] Waiting for OpenSearch on port 9200 …");
    assert!(
        wait_for_port("localhost", 9200, Duration::from_secs(300)),
        "OpenSearch did not become reachable within 300 s"
    );

    // OpenSearch might accept TCP connections via docker-proxy before the HTTP API is actually ready.
    // Poll the root endpoint until it returns a successful status.
    let opensearch_api_start = Instant::now();
    let mut opensearch_ready = false;
    let mut last_err_msg = String::new();
    while opensearch_api_start.elapsed() < Duration::from_secs(300) {
        match ureq::get(&opensearch_url)
            .set("Authorization", &opensearch_auth)
            .call()
        {
            Ok(_) => {
                opensearch_ready = true;
                break;
            }
            Err(e) => {
                last_err_msg = format!("{:?}", e);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    assert!(opensearch_ready, "OpenSearch HTTP API did not become ready in time. Last error: {}", last_err_msg);

    // ── 4. Obtain a gateway JWT via /auth/exchange ────────────────────────────
    //
    // Mirrors the pattern in scripts/send_logs.sh.
    eprintln!("[e2e] Exchanging attestation token for JWT …");
    let test_device_uuid = format!("e2e-device-{}", epoch_ms());
    let auth_body = serde_json::json!({
        "platform": "android",
        "attestation_token": "mock",
        "tenant_id": "example_tenant",
        "device_uuid": test_device_uuid,
        "device_metadata": {
            "timestamp": "2026-06-23T15:00:00Z",
            "deviceUUID": test_device_uuid,
            "sessionId": "session-123",
            "os": "Android",
            "osVersion": "13",
            "model": "Pixel 6",
            "brand": "Google",
            "device": "raven",
            "identifier": "build-1",
            "manufacturer": "Google",
            "isPhysicalDevice": "true"
        }
    });

    let auth_response: serde_json::Value = ureq::post(&format!("{gateway_url}/auth/exchange"))
        .set("Content-Type", "application/json")
        .set("X-Channel-Id", &channel)
        .send_json(auth_body)
        .expect("Auth exchange HTTP request failed")
        .into_json()
        .expect("Auth exchange response is not valid JSON");

    let jwt = auth_response["access_token"]
        .as_str()
        .expect("Auth exchange response missing 'access_token' field");

    eprintln!(
        "[e2e] JWT obtained (first 30 chars): {}…",
        &jwt[..jwt.len().min(30)]
    );

    // ── 5. POST a log with a unique message marker ────────────────────────────
    //
    // Route: /v1/logs/{channel} — the workaround path from spec report §1.
    // (The /log suffix causes a 404 on the validation worker.)
    let test_marker = format!("e2e-test-marker-{}", epoch_ms());
    eprintln!("[e2e] Sending log with unique marker: {test_marker}");

    let log_body = serde_json::json!({
        "timestamp":  "2026-06-23T15:00:00Z",
        "loggerName": "E2ETestLogger",
        "message":    test_marker,
        "level":      "INFO",
        "zone":       "main",
        "deviceUUID": test_device_uuid
    });

    let log_response = match ureq::post(&format!("{gateway_url}/v1/logs/{channel}"))
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {jwt}"))
        .send_json(log_body)
    {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            panic!("Log POST HTTP request failed with code {code}. Response body: {body}");
        }
        Err(e) => panic!("Log POST HTTP request failed: {e:?}"),
    };

    assert_eq!(
        log_response.status(),
        200,
        "Expected 200 OK from gateway log endpoint, got {}",
        log_response.status()
    );
    eprintln!("[e2e] Gateway accepted the log → 200 OK ✓");

    // ── 6. Wait for Fluent Bit to flush ──────────────────────────────────────
    //
    // fluent-bit.conf sets `Flush 2` (2-second interval). We wait 5 seconds to
    // allow for the full flush cycle plus OpenSearch indexing latency.
    eprintln!("[e2e] Waiting 5 s for Fluent Bit flush (Flush=2) + indexing …");
    std::thread::sleep(Duration::from_secs(5));

    // ── 7. Query OpenSearch for the test marker in mobile-logs-* ──────────────
    //
    // The index name is `mobile-logs-{app_id}` plus Fluent Bit's date suffix
    // (Logstash_Format=On), e.g. `mobile-logs-dev.femrek.exampleapp.dev-2026.06.24`.
    // The wildcard pattern covers all date suffixes.
    let search_url = format!(
        "{opensearch_url}/mobile-logs-*/_search?q=message:{}",
        test_marker
    );
    eprintln!("[e2e] Querying OpenSearch: {search_url}");

    let mut search_response = serde_json::Value::Null;
    let mut total_hits = 0;
    let search_start = Instant::now();

    while search_start.elapsed() < Duration::from_secs(30) {
        if let Ok(resp) = ureq::get(&search_url)
            .set("Authorization", &opensearch_auth)
            .call()
        {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                search_response = json;
                total_hits = search_response["hits"]["total"]["value"].as_u64().unwrap_or(0);
                if total_hits > 0 {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    eprintln!(
        "[e2e] OpenSearch response (hits.total): {}",
        search_response["hits"]["total"]
    );

    // ── 8. Assert document was indexed into mobile-logs-* ─────────────────────
    assert!(
        total_hits > 0,
        "Expected at least 1 document in OpenSearch with message '{}', but got 0 hits.\n\
         Full search response:\n{search_response:#}",
        test_marker
    );

    eprintln!("[e2e] ✓ Found {total_hits} document(s) in mobile-logs-*!");

    // ── 9. Query OpenSearch for the device metadata in mobile-devices-* ───────
    let device_search_url = format!(
        "{opensearch_url}/mobile-devices-*/_search?q=device_uuid:{}",
        test_device_uuid
    );
    eprintln!("[e2e] Querying OpenSearch for device metadata: {device_search_url}");

    let mut device_search_response = serde_json::Value::Null;
    let mut device_total_hits = 0;
    let device_search_start = Instant::now();

    while device_search_start.elapsed() < Duration::from_secs(30) {
        if let Ok(resp) = ureq::get(&device_search_url)
            .set("Authorization", &opensearch_auth)
            .call()
        {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                device_search_response = json;
                device_total_hits = device_search_response["hits"]["total"]["value"]
                    .as_u64()
                    .unwrap_or(0);
                if device_total_hits > 0 {
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    eprintln!(
        "[e2e] OpenSearch device metadata response (hits.total): {}",
        device_search_response["hits"]["total"]
    );

    // ── 10. Assert device metadata was indexed into mobile-devices-* ──────────
    assert!(
        device_total_hits > 0,
        "Expected at least 1 document in OpenSearch in mobile-devices-* with device_uuid '{}', but got 0 hits.\n\
         Full search response:\n{device_search_response:#}",
        test_device_uuid
    );

    let hit_source = &device_search_response["hits"]["hits"][0]["_source"];
    assert_eq!(hit_source["event_type"], "system_startup", "event_type must be system_startup");
    assert_eq!(hit_source["log_index_prefix"], "mobile-devices-dev.femrek.exampleapp.dev", "log_index_prefix must match");
    assert_eq!(hit_source["device_uuid"], test_device_uuid, "device_uuid must match");
    assert_eq!(hit_source["metadata"]["os"], "Android", "metadata.os must match");
    assert_eq!(hit_source["metadata"]["model"], "Pixel 6", "metadata.model must match");

    eprintln!("[e2e] ✓ Found {device_total_hits} device document(s) in mobile-devices-* — E2E test passed!");

    // `_stack` is dropped here → `docker compose down -v` runs automatically.
}
