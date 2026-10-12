//! Auth Worker — public API surface.

pub mod app;
pub mod auth;
pub mod config;
pub mod domain;
pub mod handler;
pub mod kafka;
pub mod metrics;

pub use app::{AppError, AppState, build_app};
pub use auth::attestation::{
    AppleAppAttestVerifier, AttestationVerifier, GooglePlayIntegrityVerifier,
    NoopVerifier, Platform, PlatformDispatchVerifier,
};
pub use config::{
    SharedWorkerConfig, WorkerChannelConfig, WorkerConfig, bootstrap as bootstrap_config,
    spawn_kv_watcher,
};
pub use domain::{
    AuthExchangeRequest, AuthExchangeResponse, DeviceMetadata, SystemStartupEvent,
};
pub use kafka::{BoxFuture, KafkaSink, KafkaSinkError, RdkafkaSink};
