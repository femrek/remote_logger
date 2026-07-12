//! Axum router construction.
//!
//! Exposed as `pub` so both the binary entry-point and integration-test
//! harnesses share the exact same routing configuration — no duplication,
//! no divergence.

use std::sync::Arc;

use axum::{routing::post, Router};

use super::state::AppState;
use crate::handler::{
    auth::auth_exchange_handler,
    ingest::{batch_ingest_handler, ingest_handler},
};

// ── Router builder ────────────────────────────────────────────────────────────

/// Build the Axum [`Router`] wired to `state`.
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // ── Auth ──────────────────────────────────────────────────────────────
        // Hardware attestation + JWT minting. Proxied here by Pingora.
        .route("/auth/exchange", post(auth_exchange_handler))
        // ── Health Check ──────────────────────────────────────────────────────
        .route("/health", axum::routing::get(|| async { axum::http::StatusCode::OK }))
        // ── Log ingestion ─────────────────────────────────────────────────────
        // Clients post raw ServerLog JSON; x-tenant-id/x-app-id injected by gateway.
        .route("/ingest", post(ingest_handler))
        .route("/v1/logs/:channel", post(ingest_handler))
        .route("/v1/logs/:channel/batch", post(batch_ingest_handler))
        .with_state(state)
}
