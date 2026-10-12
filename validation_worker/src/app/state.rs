//! Shared Axum application state for Validation Worker.

use std::sync::Arc;
use crate::kafka::KafkaSink;

/// Shared application state injected into every handler via `State<Arc<AppState>>`.
pub struct AppState {
    /// Kafka producer abstraction.
    pub sink: Arc<dyn KafkaSink>,

    /// Kafka topic for regular log ingestion (`mobile-logs`).
    pub kafka_topic: String,
}
