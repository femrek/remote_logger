//! Application layer — Axum wiring.
//!
//! Exposes:
//! - [`AppState`] — shared handler state.
//! - [`AppError`] — typed HTTP error enum.
//! - [`build_app`] — router factory used by the binary and integration tests.

mod error;
mod router;
mod state;

pub use error::AppError;
pub use router::build_app;
pub use state::AppState;
