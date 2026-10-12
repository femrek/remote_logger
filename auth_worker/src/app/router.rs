//! Axum router construction for Auth Worker.

use std::sync::Arc;
use axum::{routing::post, Router};

use super::state::AppState;
use crate::handler::auth::auth_exchange_handler;

pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/auth/exchange", post(auth_exchange_handler))
        .route("/health", axum::routing::get(|| async { axum::http::StatusCode::OK }))
        .with_state(state)
}
