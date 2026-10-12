//! Axum router construction for Validation Worker.

use std::sync::Arc;
use axum::{routing::post, Router};

use super::state::AppState;
use crate::handler::ingest::{batch_ingest_handler, ingest_handler};

/// Build the Axum [`Router`] wired to `state`.
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // ── Health Check ──────────────────────────────────────────────────────
        .route("/health", axum::routing::get(|| async { axum::http::StatusCode::OK }))
        // ── Log ingestion ─────────────────────────────────────────────────────
        // Clients post raw ServerLog JSON; x-tenant-id/x-app-id injected by gateway.
        .route("/ingest", post(ingest_handler))
        .route("/v1/logs/:channel", post(ingest_handler))
        .route("/v1/logs/:channel/batch", post(batch_ingest_handler))
        .with_state(state)
}
