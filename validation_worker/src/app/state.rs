//! Shared Axum application state.

use std::sync::Arc;

use crate::kafka::KafkaSink;

// ── Application state ─────────────────────────────────────────────────────────

/// Shared application state injected into every handler via `State<Arc<AppState>>`.
///
/// `sink` is a trait object so that tests can substitute a no-op implementation
/// without touching any handler code.
pub struct AppState {
    pub sink: Arc<dyn KafkaSink>,
    pub kafka_topic: String,
}
