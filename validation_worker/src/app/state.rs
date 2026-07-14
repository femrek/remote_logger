//! Shared Axum application state.

use std::sync::Arc;

use crate::{
    auth::attestation::AttestationVerifier,
    config::SharedWorkerConfig,
    kafka::KafkaSink,
};

// ── Application state ─────────────────────────────────────────────────────────

/// Shared application state injected into every handler via `State<Arc<AppState>>`.
///
/// `sink` and `verifier` are trait objects so that tests can substitute no-op
/// implementations without touching any handler code.
pub struct AppState {
    /// Kafka producer abstraction — shared between ingest and auth handlers.
    pub sink: Arc<dyn KafkaSink>,

    /// Kafka topic for regular log ingestion (`mobile-logs`).
    pub kafka_topic: String,

    /// Kafka topic for `SystemStartupEvent` metadata produced at auth exchange
    /// (`device-metadata`).
    pub metadata_topic: String,

    /// Hardware attestation verifier — platform-dispatching in production,
    /// `NoopVerifier` in unit tests.
    pub verifier: Arc<dyn AttestationVerifier>,

    /// Hot-reloadable worker channel configuration (attestation policy).
    pub worker_config: SharedWorkerConfig,

    /// ECDSA private key in PEM format used for JWT signing.
    pub jwt_private_key: String,

    /// Access token lifetime in seconds (default 900 = 15 min).
    pub access_ttl_seconds: u64,

    /// Refresh token lifetime in seconds (default 604 800 = 7 days).
    pub refresh_ttl_seconds: u64,

    /// Issuer claim embedded in every JWT (default `"logger-gateway"`).
    pub jwt_issuer: String,
}
