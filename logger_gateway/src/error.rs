use thiserror::Error;

/// Top-level gateway error type used throughout the proxy lifecycle.
/// Exported for use by integration tests and future crate consumers.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum GatewayError {
    // ── Routing errors ────────────────────────────────────────────────────────
    #[error("No channel found for path: {0}")]
    ChannelNotFound(String),

    #[error("Unknown channel id: {0}")]
    UnknownChannelId(String),

    // ── Auth errors ───────────────────────────────────────────────────────────
    #[error("Missing or malformed Authorization header")]
    MissingAuthHeader,

    #[error("JWT validation failed: {0}")]
    JwtInvalid(String),

    #[error("JWT secret misconfigured: {0}")]
    JwtConfig(String),

    #[error("Missing X-Channel-Id header for /auth/exchange")]
    MissingChannelHeader,

    // ── Attestation errors ────────────────────────────────────────────────────
    #[error("Attestation failed: {0}")]
    AttestationFailed(String),

    // ── Envelope errors ───────────────────────────────────────────────────────
    #[error("Payload exceeds size limit ({limit} bytes)")]
    PayloadTooLarge { limit: usize },

    #[error("Envelope JSON parse error: {0}")]
    EnvelopeParse(String),

    // ── I/O / internal errors ─────────────────────────────────────────────────
    #[error("Failed to read request body: {0}")]
    BodyRead(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Utility methods on GatewayError.
/// Exported for integration tests; suppressed in the standalone binary.
#[allow(dead_code)]
impl GatewayError {
    /// HTTP status code that should be returned to the client for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::ChannelNotFound(_) => 404,
            Self::UnknownChannelId(_) => 404,
            Self::MissingAuthHeader => 401,
            Self::JwtInvalid(_) => 401,
            Self::JwtConfig(_) => 500,
            Self::MissingChannelHeader => 400,
            Self::AttestationFailed(_) => 403,
            Self::PayloadTooLarge { .. } => 413,
            Self::EnvelopeParse(_) => 400,
            Self::BodyRead(_) => 400,
            Self::Internal(_) => 500,
        }
    }

    /// Machine-readable error code returned in the JSON response body.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::ChannelNotFound(_) => "CHANNEL_NOT_FOUND",
            Self::UnknownChannelId(_) => "CHANNEL_NOT_FOUND",
            Self::MissingAuthHeader => "MISSING_AUTH",
            Self::JwtInvalid(_) => "JWT_INVALID",
            Self::JwtConfig(_) => "INTERNAL_ERROR",
            Self::MissingChannelHeader => "MISSING_CHANNEL_HEADER",
            Self::AttestationFailed(_) => "ATTESTATION_FAILED",
            Self::PayloadTooLarge { .. } => "PAYLOAD_TOO_LARGE",
            Self::EnvelopeParse(_) => "ENVELOPE_PARSE_ERROR",
            Self::BodyRead(_) => "BODY_READ_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Render the error as a compact JSON string for the response body.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"error":"{}","code":"{}"}}"#,
            self,
            self.error_code()
        )
    }
}
