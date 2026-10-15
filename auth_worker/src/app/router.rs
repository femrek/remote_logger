//! Axum router construction for Auth Worker.

use std::sync::Arc;
use axum::{routing::{get, post}, Router};

use super::state::AppState;
use crate::handler::auth::{auth_challenge_handler, auth_enroll_handler, auth_exchange_handler};

pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/auth/challenge", get(auth_challenge_handler))
        .route("/auth/enroll", post(auth_enroll_handler))
        .route("/auth/exchange", post(auth_exchange_handler))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .with_state(state)
}
