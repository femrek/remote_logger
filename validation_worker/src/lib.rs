//! Validation Worker — public API surface.
//!
//! All business logic is implemented in the sub-modules below.
//! This file acts as a re-export facade so that:
//!   - `main.rs` can import `validation_worker::{build_app, AppState, RdkafkaSink}`.
//!   - Integration tests keep their existing `use validation_worker::…` paths.

pub mod app;
pub mod domain;
pub mod handler;
pub mod kafka;
pub mod metrics;

// ── Flat re-exports ──────────────────────────────────────────────────────────

pub use app::{AppError, AppState, build_app};
pub use domain::{KafkaPayload, ServerLog};
pub use kafka::{BoxFuture, KafkaSink, KafkaSinkError, RdkafkaSink};
