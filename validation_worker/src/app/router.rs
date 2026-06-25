//! Axum router construction.
//!
//! Exposed as `pub` so both the binary entry-point and integration-test
//! harnesses share the exact same routing configuration — no duplication,
//! no divergence.

use std::sync::Arc;

use axum::{routing::post, Router};

use super::state::AppState;
use crate::handler::ingest::ingest_handler;

// ── Router builder ────────────────────────────────────────────────────────────

/// Build the Axum [`Router`] wired to `state`.
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // Original ingest alias kept for backwards-compatibility.
        .route("/ingest", post(ingest_handler))
        // Workaround route — see spec report §1 (routing mismatch).
        // Clients MUST post to /v1/logs/{channel} without the /log suffix.
        .route("/v1/logs/:channel", post(ingest_handler))
        .with_state(state)
}
