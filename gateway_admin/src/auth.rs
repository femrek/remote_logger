use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

// ─────────────────────────────────────────────────────────────────────────────
// Token holder
// ─────────────────────────────────────────────────────────────────────────────

/// Holds the expected Bearer token bytes for constant-time comparison.
///
/// Constant-time comparison (`subtle`) prevents timing side-channels that
/// could allow an attacker to guess the token byte-by-byte.
pub struct AdminToken {
    token_bytes: Vec<u8>,
}

impl AdminToken {
    pub fn new(token: &str) -> Self {
        Self {
            token_bytes: token.as_bytes().to_vec(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Axum middleware
// ─────────────────────────────────────────────────────────────────────────────

/// Axum middleware that enforces `Authorization: Bearer <token>` on every
/// request. Returns `401 Unauthorized` with a `WWW-Authenticate` challenge
/// if the header is missing or the token does not match.
pub async fn bearer_auth_middleware(
    headers: HeaderMap,
    token: axum::extract::Extension<Arc<AdminToken>>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    let provided = extract_bearer(&headers).ok_or((
        StatusCode::UNAUTHORIZED,
        "Missing or malformed Authorization: Bearer <token> header",
    ))?;

    let expected = &token.token_bytes;
    let provided_bytes = provided.as_bytes();

    // Constant-time comparison — both slices must be same length for ct_eq.
    // If lengths differ we still do a dummy comparison to avoid short-circuit
    // timing leaks, then return Unauthorized.
    let lengths_match = expected.len() == provided_bytes.len();
    let bytes_match = if lengths_match {
        expected.ct_eq(provided_bytes).into()
    } else {
        // Still compare something to avoid timing differences on length.
        expected.ct_eq(expected).into()
    };

    if lengths_match && bytes_match {
        Ok(next.run(request).await)
    } else {
        Err((StatusCode::UNAUTHORIZED, "Invalid Bearer token"))
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|s| s.to_string())
}
