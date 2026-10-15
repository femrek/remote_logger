//! Shared Axum application state for auth worker.

use std::sync::Arc;

use crate::{
    auth::attestation::AttestationVerifier,
    config::SharedWorkerConfig,
    kafka::KafkaSink,
};

pub struct AppState {
    /// Kafka producer abstraction.
    pub sink: Arc<dyn KafkaSink>,

    /// Kafka topic for `SystemStartupEvent` metadata (`device-metadata`).
    pub metadata_topic: String,

    /// OpenSearch index prefix for device metadata (default `"mobile-devices"`).
    pub device_index_prefix: String,

    /// Hardware attestation verifier.
    pub verifier: Arc<dyn AttestationVerifier>,

    /// Hot-reloadable worker channel configuration (attestation policy).
    pub worker_config: SharedWorkerConfig,

    /// ECDSA private key in PEM format used for JWT signing.
    pub jwt_private_key: String,

    /// ECDSA public key in PEM format used for enrollment JWT verification.
    pub jwt_public_key: String,

    /// Access token lifetime in seconds (default 86 400 = 24 hours).
    pub access_ttl_seconds: u64,

    /// Issuer claim embedded in every JWT (default `"logger-gateway"`).
    pub jwt_issuer: String,
}
