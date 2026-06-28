//! Validation Worker — public API surface.
//!
//! All business logic is implemented in the sub-modules below.
//! This file acts as a re-export facade so that:
//!   - `main.rs` can import `validation_worker::{build_app, AppState, RdkafkaSink}`.
//!   - Integration tests keep their existing `use validation_worker::…` paths.

pub mod app;
pub mod auth;
pub mod config;
pub mod domain;
pub mod handler;
pub mod kafka;

// ── Flat re-exports (preserves the existing public API) ───────────────────────

pub use app::{AppError, AppState, build_app};
pub use auth::attestation::{
    AttestationVerifier, NoopVerifier, PlatformDispatchVerifier,
    GooglePlayIntegrityVerifier, AppleAppAttestVerifier, Platform,
};
pub use config::{WorkerChannelConfig, WorkerConfig, SharedWorkerConfig, bootstrap as bootstrap_config, spawn_watcher};
pub use domain::{
    AuthExchangeRequest, AuthExchangeResponse, DeviceMetadata,
    KafkaPayload, ServerLog, SystemStartupEvent,
};
pub use kafka::{BoxFuture, KafkaSink, KafkaSinkError, RdkafkaSink};
