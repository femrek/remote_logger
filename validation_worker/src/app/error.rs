//! Application-level error type and its Axum [`IntoResponse`] conversion.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

// ── Error type ────────────────────────────────────────────────────────────────

pub enum AppError {
    BadRequest,
    ServiceUnavailable,
    InternalError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest => (StatusCode::BAD_REQUEST, "Bad Request"),
            AppError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable"),
            AppError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };
        (status, msg).into_response()
    }
}
