use crate::config::JwtConfig;
use chrono::Utc;
use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────────

/// Claims embedded in every gateway-issued session JWT.
///
/// Clients receive this token from `/auth/exchange` and present it as a
/// `Bearer` token on every subsequent ingest request.  The gateway validates
/// only these claims — it never re-contacts the hardware attestation vendor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayClaims {
    /// Subject — the channel `id` this token was issued for.
    pub sub: String,
    /// Tenant / organization identifier extracted from the attestation payload.
    pub tid: String,
    /// Mobile platform the token was issued for (`"android"` | `"ios"`).
    pub plt: String,
    /// Application ID validated during the exchange.
    pub app: String,
    /// Expiry (Unix timestamp seconds).
    pub exp: usize,
    /// Issued-at (Unix timestamp seconds).
    pub iat: usize,
    /// Issuer — must match `gateway.jwt.issuer` in the config.
    pub iss: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("JWT validation failed: {0}")]
    Validation(#[from] jsonwebtoken::errors::Error),
    #[error("Could not resolve JWT secret: {0}")]
    SecretResolution(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue
// ─────────────────────────────────────────────────────────────────────────────

/// Issue a short-lived HS256 gateway JWT.
///
/// The resulting token is returned to the client at `/auth/exchange` and
/// must be presented as `Authorization: Bearer <token>` on ingest endpoints.
pub fn issue_jwt(
    channel_id: &str,
    tenant_id: &str,
    platform: &str,
    app_id: &str,
    cfg: &JwtConfig,
) -> Result<String, JwtError> {
    let secret = cfg
        .resolved_secret()
        .map_err(|e| JwtError::SecretResolution(e.to_string()))?;

    let now = Utc::now().timestamp() as usize;
    let exp = now + cfg.ttl_seconds as usize;

    let claims = GatewayClaims {
        sub: channel_id.to_owned(),
        tid: tenant_id.to_owned(),
        plt: platform.to_owned(),
        app: app_id.to_owned(),
        exp,
        iat: now,
        iss: cfg.issuer.clone(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(JwtError::Validation)
}

// ─────────────────────────────────────────────────────────────────────────────
// Validate
// ─────────────────────────────────────────────────────────────────────────────

/// Validate a `Bearer` JWT from an ingest request and return the decoded claims.
///
/// Checks performed:
/// - Signature (HS256, with the resolved secret)
/// - Expiry (`exp`)
/// - Issuer (`iss` must match `cfg.issuer`)
///
/// Intentionally does **not** re-check the hardware attestation — that step
/// happened once at `/auth/exchange` and is encoded in the signed token.
pub fn validate_jwt(token: &str, cfg: &JwtConfig) -> Result<GatewayClaims, JwtError> {
    let secret = cfg
        .resolved_secret()
        .map_err(|e| JwtError::SecretResolution(e.to_string()))?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[&cfg.issuer]);
    // exp is validated automatically by the `jsonwebtoken` crate.

    let TokenData { claims, .. } = decode::<GatewayClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(claims)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract a `Bearer <token>` value from an `Authorization` header value.
///
/// Returns `None` if the header is absent or has a non-Bearer scheme.
pub fn extract_bearer(auth_header: &str) -> Option<&str> {
    auth_header.strip_prefix("Bearer ").map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JwtConfig;

    fn test_cfg() -> JwtConfig {
        JwtConfig {
            secret: Some("test-secret-do-not-use-in-prod".to_owned()),
            secret_env: None,
            ttl_seconds: 60,
            issuer: "logger-gateway".to_owned(),
        }
    }

    #[test]
    fn roundtrip_valid_token() {
        let cfg = test_cfg();
        let token = issue_jwt("fancy-app-prod", "acme", "android", "com.test", &cfg).unwrap();
        let claims = validate_jwt(&token, &cfg).unwrap();
        assert_eq!(claims.sub, "fancy-app-prod");
        assert_eq!(claims.tid, "acme");
        assert_eq!(claims.plt, "android");
        assert_eq!(claims.app, "com.test");
        assert_eq!(claims.iss, "logger-gateway");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let cfg = test_cfg();
        let token = issue_jwt("ch", "t", "android", "com.test", &cfg).unwrap();

        let bad_cfg = JwtConfig {
            secret: Some("wrong-secret".to_owned()),
            ..test_cfg()
        };
        assert!(validate_jwt(&token, &bad_cfg).is_err());
    }

    #[test]
    fn extract_bearer_ok() {
        assert_eq!(extract_bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer("Basic abc"), None);
        assert_eq!(extract_bearer(""), None);
    }
}
