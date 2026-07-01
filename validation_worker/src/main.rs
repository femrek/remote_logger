//! Binary entry-point for the Stateless Validation Worker.
//!
//! All business logic lives in `lib.rs`. This file is intentionally minimal:
//! it reads environment variables, wires up the real [`RdkafkaSink`], builds
//! the attestation verifier, bootstraps the worker channel config, and hands
//! control to the shared [`build_app`] function.

use std::sync::Arc;
use arc_swap::ArcSwap;

use validation_worker::{
    build_app, AppState, RdkafkaSink,
    auth::attestation::{
        AppleAppAttestVerifier, GooglePlayIntegrityVerifier, PlatformDispatchVerifier,
    },
    config::{WorkerConfig, bootstrap, spawn_watcher},
    metrics,
};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    // ── Tracing ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── Prometheus metrics registration ───────────────────────────────────────
    if let Err(e) = metrics::register_all() {
        tracing::error!(error = %e, "Failed to register Prometheus metrics");
    }

    // ── Environment variables ─────────────────────────────────────────────────
    let kafka_brokers   = env_or("KAFKA_BROKERS",           "localhost:9092");
    let kafka_topic     = env_or("KAFKA_TOPIC",             "mobile-logs");
    let metadata_topic  = env_or("KAFKA_METADATA_TOPIC",    "device-metadata");
    let jwt_secret      = std::env::var("GATEWAY_JWT_SECRET")
        .expect("GATEWAY_JWT_SECRET env var must be set");
    let access_ttl      = env_u64("ACCESS_JWT_TTL_SECONDS",  900);
    let refresh_ttl     = env_u64("REFRESH_JWT_TTL_SECONDS", 604_800);
    let jwt_issuer      = env_or("JWT_ISSUER",              "logger-gateway");
    let worker_cfg_path = env_or("WORKER_CONFIG",           "config/channels.yaml");
    let port            = env_or("PORT",                    "3000");
    let metrics_port = env_or("METRICS_PORT", "9091");

    // ── Worker channel config (hot-reloadable) ────────────────────────────────
    let worker_config = match bootstrap(&worker_cfg_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!(
                path = %worker_cfg_path,
                error = %e,
                "Could not load channels.yaml — starting with empty channel list"
            );
            Arc::new(ArcSwap::from_pointee(WorkerConfig { channels: vec![] }))
        }
    };

    // Start hot-reload watcher in the background (non-fatal if it fails).
    if let Err(e) = spawn_watcher(worker_cfg_path.clone(), worker_config.clone()).await {
        tracing::warn!(error = %e, "Failed to start worker config watcher");
    }

    // ── Kafka producer ────────────────────────────────────────────────────────
    let sink = RdkafkaSink::new(&kafka_brokers).expect("Failed to create Kafka producer");

    // ── Attestation verifier ──────────────────────────────────────────────────
    // In production, replace stubs with real implementations:
    //   - GooglePlayIntegrityVerifier: call the Play Integrity API
    //   - AppleAppAttestVerifier:      decode CBOR + verify certificate chain
    let verifier = Arc::new(PlatformDispatchVerifier {
        android: Box::new(GooglePlayIntegrityVerifier),
        ios: Box::new(AppleAppAttestVerifier),
    });

    // ── Application state ─────────────────────────────────────────────────────
    let state = Arc::new(AppState {
        sink: Arc::new(sink),
        kafka_topic,
        metadata_topic,
        verifier,
        worker_config,
        jwt_secret,
        access_ttl_seconds: access_ttl,
        refresh_ttl_seconds: refresh_ttl,
        jwt_issuer,
    });

    let app = build_app(state);

    // ── Spawn Prometheus metrics server ───────────────────────────────────────
    let metrics_addr = format!("0.0.0.0:{metrics_port}");
    tokio::spawn(async move {
        if let Err(e) = metrics::serve_metrics(&metrics_addr).await {
            tracing::error!(error = %e, "Metrics server error");
        }
    });
    tracing::info!(port = %metrics_port, "Prometheus /metrics available on port {}", metrics_port);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Stateless Validation Worker listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
