//! `POST /auth/exchange` handler.
//!
//! ## Processing pipeline
//!
//! ```text
//! 1. Read X-Channel-Id header → look up WorkerChannelConfig → 400/404
//! 2. Deserialize AuthExchangeRequest from JSON body → 400 on failure
//! 3. If channel.require_apple_key_hash && platform == ios
//!      → device_public_key must be present → 400 if missing
//! 4. If channel.validate_attestation
//!      → PlatformDispatchVerifier::verify(...) → 403 on failure
//! 5. Generate session_id (UUID v4) server-side
//! 6. Mint access JWT  (token_type = "access",  TTL = access_ttl_seconds)
//! 7. Mint refresh JWT (token_type = "refresh", TTL = refresh_ttl_seconds,
//!      apk_hash = hex(sha256(device_public_key)) if enabled)
//! 8. Serialize SystemStartupEvent → Kafka topic device-metadata
//! 9. Return AuthExchangeResponse { access_token, refresh_token,
//!      access_expires_in, refresh_expires_in }
//! ```

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::HeaderMap,
    response::IntoResponse,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app::{AppError, AppState},
    auth::{
        attestation::Platform,
        jwt::{JwtConfig, issue_access_jwt, issue_refresh_jwt, sha256_hex},
    },
    domain::{AuthExchangeRequest, AuthExchangeResponse, SystemStartupEvent},
    metrics::JWT_OPERATIONS_TOTAL,
};

// ── Request handler ───────────────────────────────────────────────────────────

/// `POST /auth/exchange` handler.
pub async fn auth_exchange_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<AuthExchangeRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    // ── 1. Resolve channel from X-Channel-Id header ───────────────────────────
    let channel_id = headers
        .get("x-channel-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing or empty X-Channel-Id header");
            AppError::BadRequest("X-Channel-Id header is required".to_string())
        })?
        .to_owned();

    let cfg = state.worker_config.load();
    let channel = cfg.find_channel_by_id(&channel_id).ok_or_else(|| {
        tracing::warn!(channel_id = %channel_id, "Unknown channel");
        AppError::NotFound(format!("Unknown channel: {channel_id}"))
    })?;

    let app_id = channel.app_id.clone();
    let validate_attestation = channel.validate_attestation;
    let require_apple_key_hash = channel.require_apple_key_hash;
    let google_project_number = channel.google_project_number.clone();
    let apple_team_id = channel.apple_team_id.clone();
    // Drop config guard before any async work
    drop(cfg);

    // ── 2. Parse request body ─────────────────────────────────────────────────
    let Json(req) = match payload {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse AuthExchangeRequest body");
            return Err(AppError::BadRequest(format!("Invalid request body: {err}")));
        }
    };

    // ── 3. Apple public key requirement check ─────────────────────────────────
    if require_apple_key_hash && req.platform == Platform::Ios {
        if req.device_public_key.is_none() {
            tracing::warn!(
                channel = %channel_id,
                "iOS channel requires device_public_key but it was absent"
            );
            return Err(AppError::BadRequest(
                "device_public_key is required for this iOS channel".to_string(),
            ));
        }
    }

    // ── 4. Attestation verification ───────────────────────────────────────────
    if validate_attestation {
        if let Err(e) = state
            .verifier
            .verify(
                &req.attestation_token,
                req.platform,
                &app_id,
                google_project_number.as_deref(),
                apple_team_id.as_deref(),
            )
            .await
        {
            tracing::warn!(
                channel = %channel_id,
                platform = %req.platform,
                error = %e,
                "Attestation failed"
            );
            return Err(AppError::AttestationFailed(e.to_string()));
        }
        tracing::info!(channel = %channel_id, "Attestation passed");
    } else {
        tracing::debug!(channel = %channel_id, "Attestation skipped (disabled for this channel)");
    }

    // ── 5. Generate session_id server-side ────────────────────────────────────
    let session_id = Uuid::new_v4().to_string();

    // ── 6 & 7. Compute apk_hash then mint both tokens ─────────────────────────
    let apk_hash: Option<String> = if require_apple_key_hash
        && req.platform == Platform::Ios
    {
        // device_public_key presence already enforced in step 3.
        let key_b64 = req.device_public_key.as_deref().unwrap_or_default();
        match BASE64.decode(key_b64) {
            Ok(key_bytes) => Some(sha256_hex(&key_bytes)),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to base64-decode device_public_key");
                return Err(AppError::BadRequest(
                    "device_public_key must be base64-encoded".to_string(),
                ));
            }
        }
    } else {
        None
    };

    let jwt_cfg = JwtConfig {
        private_key: &state.jwt_private_key,
        access_ttl_seconds: state.access_ttl_seconds,
        refresh_ttl_seconds: state.refresh_ttl_seconds,
        issuer: &state.jwt_issuer,
    };

    let access_token = match issue_access_jwt(
        &channel_id,
        &req.platform.to_string(),
        &app_id,
        &req.device_uuid,
        &session_id,
        &jwt_cfg,
    ) {
        Ok(t) => {
            JWT_OPERATIONS_TOTAL.with_label_values(&["mint_access", "ok"]).inc();
            t
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to mint access JWT");
            JWT_OPERATIONS_TOTAL.with_label_values(&["mint_access", "err"]).inc();
            return Err(AppError::InternalError);
        }
    };

    let refresh_token = match issue_refresh_jwt(
        &channel_id,
        &req.platform.to_string(),
        &app_id,
        &req.device_uuid,
        &session_id,
        apk_hash,
        &jwt_cfg,
    ) {
        Ok(t) => {
            JWT_OPERATIONS_TOTAL.with_label_values(&["mint_refresh", "ok"]).inc();
            t
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to mint refresh JWT");
            JWT_OPERATIONS_TOTAL.with_label_values(&["mint_refresh", "err"]).inc();
            return Err(AppError::InternalError);
        }
    };

    // ── 8. Produce SystemStartupEvent to Kafka ────────────────────────────────
    let startup_event = SystemStartupEvent {
        event_type: "system_startup".to_string(),
        app_id: app_id.clone(),
        device_uuid: req.device_uuid.clone(),
        session_id: session_id.clone(),
        platform: req.platform.to_string(),
        metadata: req.device_metadata,
        ingested_at: Utc::now().to_rfc3339(),
    };

    let serialised = serde_json::to_string(&startup_event).map_err(|e| {
        tracing::error!(error = %e, "Failed to serialise SystemStartupEvent");
        AppError::InternalError
    })?;

    match state
        .sink
        .send(&state.metadata_topic, &req.device_uuid, &serialised)
        .await
    {
        Ok(()) => tracing::info!(
            channel = %channel_id,
            device_uuid = %req.device_uuid,
            "SystemStartupEvent produced to Kafka"
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to deliver SystemStartupEvent to Kafka");
            return Err(AppError::ServiceUnavailable);
        }
    }

    // ── 9. Return response ────────────────────────────────────────────────────
    tracing::info!(
        channel = %channel_id,
        platform = %req.platform,
        device_uuid = %req.device_uuid,
        session_id = %session_id,
        "Auth exchange successful"
    );

    Ok(Json(AuthExchangeResponse {
        access_token,
        refresh_token,
        access_expires_in: state.access_ttl_seconds,
        refresh_expires_in: state.refresh_ttl_seconds,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use base64::Engine;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::{
        app::{AppState, build_app},
        auth::attestation::{NoopVerifier, PlatformDispatchVerifier},
        config::{WorkerChannelConfig, WorkerConfig},
        kafka::{BoxFuture, KafkaSink, KafkaSinkError},
    };
    use arc_swap::ArcSwap;

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

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

    fn make_state(channels: Vec<WorkerChannelConfig>) -> Arc<AppState> {
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig { channels }));
        Arc::new(AppState {
            sink: Arc::new(NoopSink),
            kafka_topic: "mobile-logs".to_string(),
            metadata_topic: "device-metadata".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(NoopVerifier),
                ios: Box::new(NoopVerifier),
            }),
            worker_config,
            jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
            access_ttl_seconds: 900,
            refresh_ttl_seconds: 604_800,
            jwt_issuer: "logger-gateway".to_string(),
        })
    }

    fn dev_channel() -> WorkerChannelConfig {
        WorkerChannelConfig {
            id: "example-app-dev".to_string(),
            app_id: "dev.femrek.exampleapp.dev".to_string(),
            validate_attestation: false,
            google_project_number: None,
            apple_team_id: None,
            require_apple_key_hash: false,
        }
    }

    fn ios_channel_with_key_hash() -> WorkerChannelConfig {
        WorkerChannelConfig {
            id: "example-ios-prod".to_string(),
            app_id: "dev.femrek.exampleapp".to_string(),
            validate_attestation: false,
            google_project_number: None,
            apple_team_id: Some("STUBTEAMID".to_string()),
            require_apple_key_hash: true,
        }
    }

    fn valid_android_body(_channel_id: &str) -> serde_json::Value {
        serde_json::json!({
            "platform": "android",
            "attestation_token": "fake-android-token",
            "tenant_id": "example_tenant",
            "device_uuid": "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "device_metadata": {
                "timestamp": "2026-06-28T17:00:00Z",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
                "sessionId": "3f338d38-9588-46d4-83b6-96d5395a12ef",
                "os": "Android",
                "osVersion": "13",
                "model": "Pixel 6 Pro",
                "brand": "Google",
                "device": "raven",
                "identifier": "TQ3A.230605.012",
                "manufacturer": "Google",
                "isPhysicalDevice": "true"
            }
        })
    }

    async fn post_exchange(
        app: axum::Router,
        channel_id: &str,
        body: serde_json::Value,
    ) -> http::Response<Body> {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/exchange")
                .header("content-type", "application/json")
                .header("x-channel-id", channel_id)
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn happy_path_android_returns_200_with_tokens() {
        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);
        let resp = post_exchange(app, "example-app-dev", valid_android_body("example-app-dev")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body_bytes = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["access_expires_in"], 900);
        assert_eq!(json["refresh_expires_in"], 604_800);
    }

    #[tokio::test]
    async fn missing_channel_id_header_returns_400() {
        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/exchange")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&valid_android_body("example-app-dev")).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_channel_returns_404() {
        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);
        let resp =
            post_exchange(app, "no-such-channel", valid_android_body("no-such-channel")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_device_public_key_for_ios_key_hash_channel_returns_400() {
        let state = make_state(vec![ios_channel_with_key_hash()]);
        let app = build_app(state);

        let body = serde_json::json!({
            "platform": "ios",
            "attestation_token": "fake-ios-token",
            "tenant_id": "example_tenant",
            "device_uuid": "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "device_metadata": {
                "timestamp": "2026-06-28T17:00:00Z",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
                "sessionId": "3f338d38-9588-46d4-83b6-96d5395a12ef",
                "os": "iOS",
                "osVersion": "17.0",
                "model": "iPhone 15 Pro",
                "brand": "Apple",
                "device": "iPhone16,1",
                "identifier": "23A344",
                "manufacturer": "Apple",
                "isPhysicalDevice": "true"
            }
            // device_public_key intentionally absent
        });

        let resp = post_exchange(app, "example-ios-prod", body).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ios_with_key_hash_and_valid_key_returns_200() {
        let state = make_state(vec![ios_channel_with_key_hash()]);
        let app = build_app(state);

        // Base64-encode some fake key bytes
        let fake_key = base64::engine::general_purpose::STANDARD.encode(b"fake-apple-public-key");

        let body = serde_json::json!({
            "platform": "ios",
            "attestation_token": "fake-ios-token",
            "tenant_id": "example_tenant",
            "device_uuid": "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "device_metadata": {
                "timestamp": "2026-06-28T17:00:00Z",
                "deviceUUID": "a809b022-7934-4b53-90d2-bf7cb95a947d",
                "sessionId": "3f338d38-9588-46d4-83b6-96d5395a12ef",
                "os": "iOS",
                "osVersion": "17.0",
                "model": "iPhone 15 Pro",
                "brand": "Apple",
                "device": "iPhone16,1",
                "identifier": "23A344",
                "manufacturer": "Apple",
                "isPhysicalDevice": "true"
            },
            "device_public_key": fake_key
        });

        let resp = post_exchange(app, "example-ios-prod", body).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_json_body_returns_400() {
        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/exchange")
                    .header("content-type", "application/json")
                    .header("x-channel-id", "example-app-dev")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
