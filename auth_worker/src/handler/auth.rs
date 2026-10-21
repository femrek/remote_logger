//! `POST /auth/exchange` handler for Auth Worker.

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::HeaderMap,
    response::IntoResponse,
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    app::{AppError, AppState},
    auth::jwt::{JwtConfig, issue_access_jwt},
    domain::{AuthExchangeRequest, AuthExchangeResponse, SystemStartupEvent},
    metrics::JWT_OPERATIONS_TOTAL,
};

/// `GET /auth/challenge` handler.
pub async fn auth_challenge_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let channel_id = headers
        .get("x-channel-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing or empty X-Channel-Id header");
            AppError::BadRequest("X-Channel-Id header is required".to_string())
        })?;

    let cfg = state.worker_config.load();
    if cfg.find_channel_by_id(channel_id).is_none() {
        tracing::warn!(channel_id = %channel_id, "Unknown channel");
        return Err(AppError::NotFound(format!("Unknown channel: {channel_id}")));
    }
    drop(cfg);

    let challenge = crate::auth::jwt::create_challenge(&state.jwt_private_key, 300);
    Ok(Json(crate::domain::AuthChallengeResponse {
        challenge,
        expires_in: 300,
    }))
}

/// `POST /auth/enroll` handler.
pub async fn auth_enroll_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Result<Json<crate::domain::AuthEnrollRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    let channel_id = headers
        .get("x-channel-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            tracing::warn!("Missing or empty X-Channel-Id header");
            AppError::BadRequest("X-Channel-Id header is required".to_string())
        })?
        .to_owned();

    let Json(req) = match payload {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse AuthEnrollRequest body");
            return Err(AppError::BadRequest(format!("Invalid request body: {err}")));
        }
    };

    let cfg = state.worker_config.load();
    let channel = cfg.find_channel_by_id(&channel_id).ok_or_else(|| {
        tracing::warn!(channel_id = %channel_id, "Unknown channel");
        AppError::NotFound(format!("Unknown channel: {channel_id}"))
    })?;

    let app_id = channel.app_id.clone();
    let android_signing_cert_sha256 = channel.android_signing_cert_sha256.clone();
    let apple_team_id = channel.apple_team_id.clone();
    let apple_app_attest_env = channel.apple_app_attest_env.clone();
    let att_mode = channel.mode_for_platform(req.platform);
    drop(cfg);

    // Verify challenge freshness & signature
    if let Err(err) = crate::auth::jwt::verify_challenge(&req.challenge, &state.jwt_private_key) {
        tracing::warn!(channel_id = %channel_id, error = %err, "Invalid or expired challenge");
        return Err(AppError::AttestationFailed(format!("Invalid challenge: {err}")));
    }

    let att_ctx = crate::auth::attestation::AttestationContext {
        token: &req.attestation,
        platform: req.platform,
        app_id: &app_id,
        challenge: Some(&req.challenge),
        public_key: Some(&req.public_key),
        android_signing_cert_sha256: &android_signing_cert_sha256,
        apple_team_id: apple_team_id.as_deref(),
        apple_app_attest_env: apple_app_attest_env.as_deref(),
    };

    // Verify attestation statement based on platform mode
    let att = match att_mode {
        crate::config::AttestationMode::Off => {
            tracing::debug!(channel = %channel_id, "Attestation skipped in enrollment (off)");
            "none"
        }
        crate::config::AttestationMode::Monitor => match state.verifier.verify(att_ctx).await {
            Ok(tier) => {
                tracing::info!(
                    channel = %channel_id,
                    tier = %tier,
                    "Attestation passed in monitor enrollment"
                );
                tier.as_str()
            }
            Err(e) => {
                tracing::warn!(
                    channel = %channel_id,
                    error = %e,
                    "Attestation failed in monitor enrollment; minting enrollment with att=none"
                );
                "none"
            }
        },
        crate::config::AttestationMode::Enforce => {
            let tier = state.verifier.verify(att_ctx).await.map_err(|e| {
                tracing::warn!(
                    channel = %channel_id,
                    error = %e,
                    "Attestation failed in enforce enrollment"
                );
                AppError::AttestationFailed(e.to_string())
            })?;
            tracing::info!(
                channel = %channel_id,
                tier = %tier,
                "Attestation passed in enforce enrollment"
            );
            tier.as_str()
        }
    };

    let enroll_ttl_seconds = 7_776_000u64; // 90 days
    let jwt_cfg = JwtConfig {
        private_key: &state.jwt_private_key,
        access_ttl_seconds: enroll_ttl_seconds,
        issuer: &state.jwt_issuer,
    };

    let enrollment_token = crate::auth::jwt::issue_enrollment_jwt(
        &req.platform.to_string(),
        &app_id,
        &req.device_id,
        &req.public_key,
        att,
        enroll_ttl_seconds,
        &jwt_cfg,
    )
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to mint enrollment JWT");
        AppError::InternalError
    })?;

    Ok(Json(crate::domain::AuthEnrollResponse {
        enrollment_token,
        expires_in: enroll_ttl_seconds,
    }))
}

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

    // ── 2. Parse request body ─────────────────────────────────────────────────
    let Json(req) = match payload {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to parse AuthExchangeRequest body");
            return Err(AppError::BadRequest(format!("Invalid request body: {err}")));
        }
    };

    // ── 3. Resolve channel config and platform attestation mode ───────────────
    let cfg = state.worker_config.load();
    let channel = cfg.find_channel_by_id(&channel_id).ok_or_else(|| {
        tracing::warn!(channel_id = %channel_id, "Unknown channel");
        AppError::NotFound(format!("Unknown channel: {channel_id}"))
    })?;

    let app_id = channel.app_id.clone();
    let android_signing_cert_sha256 = channel.android_signing_cert_sha256.clone();
    let apple_team_id = channel.apple_team_id.clone();
    let apple_app_attest_env = channel.apple_app_attest_env.clone();
    let att_mode = channel.mode_for_platform(req.platform);
    drop(cfg);

    // ── 4. Attestation verification (enrollment proof or direct) ──────────────
    let att = if let Some(ref enrollment_token) = req.enrollment_token {
        let challenge = req.challenge.as_deref().ok_or_else(|| {
            AppError::BadRequest("challenge required when enrollment_token is provided".into())
        })?;
        let challenge_sig_b64 = req.challenge_signature.as_deref().ok_or_else(|| {
            AppError::BadRequest("challenge_signature required when enrollment_token is provided".into())
        })?;

        // 1. Verify challenge
        if let Err(e) = crate::auth::jwt::verify_challenge(challenge, &state.jwt_private_key) {
            tracing::warn!(channel = %channel_id, error = %e, "Session challenge verification failed");
            return Err(AppError::AttestationFailed(format!("Invalid challenge: {e}")));
        }

        // 2. Decode enrollment token
        let claims = crate::auth::jwt::decode_enrollment_jwt(
            enrollment_token,
            &state.jwt_public_key,
            &state.jwt_issuer,
        )
        .map_err(|e| {
            tracing::warn!(channel = %channel_id, error = %e, "Invalid enrollment token");
            AppError::AttestationFailed(format!("Invalid enrollment token: {e}"))
        })?;

        // Validate claims match request
        if claims.app != app_id || claims.sub != req.device_id || claims.plt != req.platform.to_string() {
            tracing::warn!(channel = %channel_id, "Enrollment token claim mismatch");
            return Err(AppError::AttestationFailed("Enrollment token claim mismatch".into()));
        }

        // 3. Verify challenge signature using enrolled public key
        use base64::Engine;
        let pub_key_bytes = base64::engine::general_purpose::STANDARD
            .decode(&claims.pub_key)
            .map_err(|e| {
                tracing::warn!(error = %e, "Malformed base64 pub_key in enrollment claims");
                AppError::AttestationFailed("Malformed enrolled public key".into())
            })?;
        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(challenge_sig_b64)
            .map_err(|e| {
                tracing::warn!(error = %e, "Malformed base64 challenge_signature");
                AppError::BadRequest("Malformed base64 challenge_signature".into())
            })?;

        if let Err(e) = crate::auth::jwt::verify_p256_signature(
            &pub_key_bytes,
            challenge.as_bytes(),
            &sig_bytes,
        ) {
            tracing::warn!(channel = %channel_id, error = %e, "Challenge signature verification failed");
            return Err(AppError::AttestationFailed(format!("Challenge signature verification failed: {e}")));
        }

        tracing::info!(channel = %channel_id, att = %claims.att, "Session proof verified successfully via enrollment token");
        match claims.att.as_str() {
            "tee" => "tee",
            "strongbox" => "strongbox",
            "secure_enclave" => "secure_enclave",
            _ => "none",
        }
    } else {
        let att_ctx = crate::auth::attestation::AttestationContext {
            token: &req.attestation_token,
            platform: req.platform,
            app_id: &app_id,
            challenge: req.challenge.as_deref(),
            public_key: req.device_public_key.as_deref(),
            android_signing_cert_sha256: &android_signing_cert_sha256,
            apple_team_id: apple_team_id.as_deref(),
            apple_app_attest_env: apple_app_attest_env.as_deref(),
        };

        match att_mode {
            crate::config::AttestationMode::Off => {
                tracing::debug!(channel = %channel_id, "Attestation skipped (off for this platform)");
                "none"
            }
            crate::config::AttestationMode::Monitor => match state.verifier.verify(att_ctx).await {
                Ok(tier) => {
                    tracing::info!(
                        channel = %channel_id,
                        tier = %tier,
                        "Attestation passed in monitor mode"
                    );
                    tier.as_str()
                }
                Err(e) => {
                    tracing::warn!(
                        channel = %channel_id,
                        platform = %req.platform,
                        error = %e,
                        "Attestation failed in monitor mode; granting access with att=none"
                    );
                    "none"
                }
            },
            crate::config::AttestationMode::Enforce => {
                let tier = state.verifier.verify(att_ctx).await.map_err(|e| {
                    tracing::warn!(
                        channel = %channel_id,
                        platform = %req.platform,
                        error = %e,
                        "Attestation failed in enforce mode"
                    );
                    AppError::AttestationFailed(e.to_string())
                })?;
                tracing::info!(
                    channel = %channel_id,
                    tier = %tier,
                    "Attestation passed in enforce mode"
                );
                tier.as_str()
            }
        }
    };

    // ── 5. Resolve session_id (honor client session ID if provided) ───────────
    let session_id = if !req.device_metadata.session_id.trim().is_empty() {
        req.device_metadata.session_id.clone()
    } else {
        Uuid::new_v4().to_string()
    };

    // ── 6. Mint access token ──────────────────────────────────────────────────
    let jwt_cfg = JwtConfig {
        private_key: &state.jwt_private_key,
        access_ttl_seconds: state.access_ttl_seconds,
        issuer: &state.jwt_issuer,
    };

    let access_token = match issue_access_jwt(
        &req.platform.to_string(),
        &app_id,
        &req.device_id,
        &session_id,
        att,
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

    // ── 7. Produce SystemStartupEvent to Kafka ────────────────────────────────
    let startup_event = SystemStartupEvent {
        log_index_prefix: format!("{}-{}", state.device_index_prefix, app_id),
        event_type: "system_startup".to_string(),
        app_id: app_id.clone(),
        device_id: req.device_id.clone(),
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
        .send(&state.metadata_topic, &req.device_id, &serialised)
        .await
    {
        Ok(()) => tracing::info!(
            channel = %channel_id,
            device_id = %req.device_id,
            "SystemStartupEvent produced to Kafka"
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to deliver SystemStartupEvent to Kafka");
            return Err(AppError::ServiceUnavailable);
        }
    }

    // ── 8. Return response ────────────────────────────────────────────────────
    tracing::info!(
        channel = %channel_id,
        platform = %req.platform,
        device_id = %req.device_id,
        session_id = %session_id,
        att = %att,
        "Auth exchange successful"
    );

    Ok(Json(AuthExchangeResponse {
        access_token,
        access_expires_in: state.access_ttl_seconds,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::{
        app::{AppState, build_app},
        auth::attestation::{
            AttestationContext, AttestationError, AttestationVerifier, NoopVerifier,
            PlatformDispatchVerifier,
        },
        config::{AttestationMode, WorkerChannelConfig, WorkerConfig},
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

    struct FailingVerifier;
    impl AttestationVerifier for FailingVerifier {
        fn verify<'a>(
            &'a self,
            _ctx: AttestationContext<'a>,
        ) -> crate::auth::attestation::VerifyFuture<'a> {
            Box::pin(async { Err(AttestationError::Invalid("simulated failure".into())) })
        }
    }

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

    fn make_state(channels: Vec<WorkerChannelConfig>) -> Arc<AppState> {
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig { channels }));
        let jwt_public_key = crate::auth::jwt::derive_public_key_pem(TEST_PRIVATE_KEY_PEM).unwrap();
        Arc::new(AppState {
            sink: Arc::new(NoopSink),
            metadata_topic: "device-metadata".to_string(),
            device_index_prefix: "mobile-devices".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(NoopVerifier),
                ios: Box::new(NoopVerifier),
            }),
            worker_config,
            jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
            jwt_public_key,
            access_ttl_seconds: 900,
            jwt_issuer: "logger-gateway".to_string(),
        })
    }

    fn dev_channel() -> WorkerChannelConfig {
        WorkerChannelConfig {
            id: "example-app-dev".to_string(),
            app_id: "dev.femrek.exampleapp.dev".to_string(),
            android_attestation_mode: Some(AttestationMode::Off),
            ios_attestation_mode: Some(AttestationMode::Off),
            android_signing_cert_sha256: vec![],
            apple_team_id: None,
            apple_app_attest_env: None,
        }
    }

    fn enforce_channel() -> WorkerChannelConfig {
        WorkerChannelConfig {
            id: "example-enforce".to_string(),
            app_id: "dev.femrek.exampleapp".to_string(),
            android_attestation_mode: Some(AttestationMode::Enforce),
            ios_attestation_mode: Some(AttestationMode::Enforce),
            android_signing_cert_sha256: vec![],
            apple_team_id: Some("STUBTEAMID".to_string()),
            apple_app_attest_env: None,
        }
    }

    fn monitor_channel() -> WorkerChannelConfig {
        WorkerChannelConfig {
            id: "example-monitor".to_string(),
            app_id: "dev.femrek.exampleapp".to_string(),
            android_attestation_mode: Some(AttestationMode::Monitor),
            ios_attestation_mode: Some(AttestationMode::Monitor),
            android_signing_cert_sha256: vec![],
            apple_team_id: Some("STUBTEAMID".to_string()),
            apple_app_attest_env: None,
        }
    }

    fn valid_android_body(_channel_id: &str) -> serde_json::Value {
        serde_json::json!({
            "platform": "android",
            "attestation_token": "fake-android-token",
            "tenant_id": "example_tenant",
            "device_id": "a809b022-7934-4b53-90d2-bf7cb95a947d",
            "device_metadata": {
                "timestamp": "2026-06-28T17:00:00Z",
                "deviceId": "a809b022-7934-4b53-90d2-bf7cb95a947d",
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
    async fn happy_path_android_returns_200_with_access_token() {
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
        assert!(json.get("refresh_token").is_none());
        assert_eq!(json["access_expires_in"], 900);
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
    async fn enforce_mode_fails_when_attestation_fails() {
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig {
            channels: vec![enforce_channel()],
        }));
        let jwt_public_key = crate::auth::jwt::derive_public_key_pem(TEST_PRIVATE_KEY_PEM).unwrap();
        let state = Arc::new(AppState {
            sink: Arc::new(NoopSink),
            metadata_topic: "device-metadata".to_string(),
            device_index_prefix: "mobile-devices".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(FailingVerifier),
                ios: Box::new(FailingVerifier),
            }),
            worker_config,
            jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
            jwt_public_key: jwt_public_key.clone(),
            access_ttl_seconds: 900,
            jwt_issuer: "logger-gateway".to_string(),
        });
        let app = build_app(state);

        let resp = post_exchange(app, "example-enforce", valid_android_body("example-enforce")).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn monitor_mode_succeeds_even_when_attestation_fails() {
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig {
            channels: vec![monitor_channel()],
        }));
        let jwt_public_key = crate::auth::jwt::derive_public_key_pem(TEST_PRIVATE_KEY_PEM).unwrap();
        let state = Arc::new(AppState {
            sink: Arc::new(NoopSink),
            metadata_topic: "device-metadata".to_string(),
            device_index_prefix: "mobile-devices".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(FailingVerifier),
                ios: Box::new(FailingVerifier),
            }),
            worker_config,
            jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
            jwt_public_key,
            access_ttl_seconds: 900,
            jwt_issuer: "logger-gateway".to_string(),
        });
        let app = build_app(state);

        let resp = post_exchange(app, "example-monitor", valid_android_body("example-monitor")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body_bytes = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(json["access_token"].is_string());
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
    async fn auth_exchange_produces_startup_event_with_device_index_prefix() {
        let capturing_sink = Arc::new(CapturingSink::default());
        let worker_config = Arc::new(ArcSwap::from_pointee(WorkerConfig {
            channels: vec![dev_channel()],
        }));
        let jwt_public_key = crate::auth::jwt::derive_public_key_pem(TEST_PRIVATE_KEY_PEM).unwrap();
        let state = Arc::new(AppState {
            sink: capturing_sink.clone(),
            metadata_topic: "device-metadata".to_string(),
            device_index_prefix: "mobile-devices".to_string(),
            verifier: Arc::new(PlatformDispatchVerifier {
                android: Box::new(NoopVerifier),
                ios: Box::new(NoopVerifier),
            }),
            worker_config,
            jwt_private_key: TEST_PRIVATE_KEY_PEM.to_string(),
            jwt_public_key,
            access_ttl_seconds: 900,
            jwt_issuer: "logger-gateway".to_string(),
        });
        let app = build_app(state);
        let resp = post_exchange(app, "example-app-dev", valid_android_body("example-app-dev")).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let sent = capturing_sink.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (topic, key, payload) = &sent[0];
        assert_eq!(topic, "device-metadata");
        assert_eq!(key, "a809b022-7934-4b53-90d2-bf7cb95a947d");

        let event: serde_json::Value = serde_json::from_str(payload).expect("Valid JSON");
        assert_eq!(event["log_index_prefix"], "mobile-devices-dev.femrek.exampleapp.dev");
        assert_eq!(event["event_type"], "system_startup");
        assert_eq!(event["app_id"], "dev.femrek.exampleapp.dev");
        assert_eq!(event["device_id"], "a809b022-7934-4b53-90d2-bf7cb95a947d");
        assert_eq!(event["metadata"]["os"], "Android");
        assert_eq!(event["metadata"]["model"], "Pixel 6 Pro");
    }

    #[tokio::test]
    async fn challenge_endpoint_returns_challenge_and_expires_in() {
        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/auth/challenge")
                    .header("x-channel-id", "example-app-dev")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let challenge_str = json["challenge"].as_str().unwrap();
        assert!(challenge_str.contains('.'));
        assert!(crate::auth::jwt::verify_challenge(challenge_str, TEST_PRIVATE_KEY_PEM).is_ok());
        assert_eq!(json["expires_in"], 300);
    }

    #[tokio::test]
    async fn enroll_and_exchange_with_signature_roundtrip() {
        use base64::Engine;
        use ring::signature::KeyPair;

        let state = make_state(vec![dev_channel()]);
        let app = build_app(state);

        // 1. Get challenge
        let challenge = crate::auth::jwt::create_challenge(TEST_PRIVATE_KEY_PEM, 300);

        // 2. Generate a client EC P-256 key pair
        let der = base64::engine::general_purpose::STANDARD
            .decode(
                TEST_PRIVATE_KEY_PEM
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.starts_with("-----"))
                    .collect::<String>(),
            )
            .unwrap();
        let rng = ring::rand::SystemRandom::new();
        let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &der,
            &rng,
        )
        .unwrap();

        let client_pub_b64 = base64::engine::general_purpose::STANDARD.encode(key_pair.public_key().as_ref());

        // 3. Call /auth/enroll
        let enroll_body = serde_json::json!({
            "platform": "android",
            "device_id": "client-device-42",
            "public_key": client_pub_b64,
            "attestation": "test-attestation-statement",
            "challenge": challenge,
        });

        let enroll_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/enroll")
                    .header("content-type", "application/json")
                    .header("x-channel-id", "example-app-dev")
                    .body(Body::from(serde_json::to_vec(&enroll_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(enroll_resp.status(), StatusCode::OK);
        let body_bytes = http_body_util::BodyExt::collect(enroll_resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        let enroll_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let enrollment_token = enroll_json["enrollment_token"].as_str().unwrap();

        // 4. Create fresh session challenge & sign it with client key
        let session_challenge = crate::auth::jwt::create_challenge(TEST_PRIVATE_KEY_PEM, 300);
        let sig = key_pair.sign(&rng, session_challenge.as_bytes()).unwrap();
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.as_ref());

        // 5. Exchange with enrollment_token + signature
        let mut exchange_body = valid_android_body("example-app-dev");
        exchange_body["device_id"] = serde_json::json!("client-device-42");
        exchange_body["enrollment_token"] = serde_json::json!(enrollment_token);
        exchange_body["challenge"] = serde_json::json!(session_challenge);
        exchange_body["challenge_signature"] = serde_json::json!(sig_b64);

        let exchange_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/exchange")
                    .header("content-type", "application/json")
                    .header("x-channel-id", "example-app-dev")
                    .body(Body::from(serde_json::to_vec(&exchange_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(exchange_resp.status(), StatusCode::OK);

        // 6. Test with tampered signature -> returns FORBIDDEN
        let mut bad_exchange_body = exchange_body.clone();
        bad_exchange_body["challenge_signature"] = serde_json::json!(base64::engine::general_purpose::STANDARD.encode(b"bad-signature-bytes-not-matching"));

        let bad_resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/exchange")
                    .header("content-type", "application/json")
                    .header("x-channel-id", "example-app-dev")
                    .body(Body::from(serde_json::to_vec(&bad_exchange_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(bad_resp.status(), StatusCode::FORBIDDEN);
    }
}
