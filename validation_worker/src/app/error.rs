//! Application-level error type and its Axum [`IntoResponse`] conversion.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub enum AppError {
    /// 400 — malformed request body or missing required fields.
    BadRequest(String),
    /// 401 — missing gateway-injected headers (authentication bypassed).
    Unauthorized,
    /// 503 — Kafka delivery failure.
    ServiceUnavailable,
    /// 500 — unexpected internal error.
    InternalError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "x-tenant-id and x-app-id headers are required".to_string(),
            ),
            AppError::ServiceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                "Kafka delivery failed — please retry".to_string(),
            ),
            AppError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "An internal error occurred".to_string(),
            ),
        };

        (status, Json(json!({ "error": message, "code": code }))).into_response()
    }
}
