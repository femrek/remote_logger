use crate::config::JwtConfig;
use jsonwebtoken::{
    decode, Algorithm, DecodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────────

/// Claims embedded in every gateway-issued session JWT.
///
/// Clients receive this token from `/auth/exchange` (now handled by the
/// validation worker) and present it as a `Bearer` token on every subsequent
/// ingest request. The gateway validates only these claims — it never contacts
/// the hardware attestation vendor.
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
    /// Device UUID embedded by the worker at mint time.
    pub device_uuid: String,
    /// Session ID embedded by the worker at mint time.
    pub session_id: String,
    /// Token type — `"access"` tokens are valid on ingest routes.
    pub token_type: String,
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
    #[error("Could not resolve JWT public key: {0}")]
    PublicKeyResolution(String),
    #[error("Token type is not 'access'")]
    WrongTokenType,
}

// ─────────────────────────────────────────────────────────────────────────────
// Validate
// ─────────────────────────────────────────────────────────────────────────────

/// Validate a `Bearer` JWT from an ingest request and return the decoded claims.
///
/// Checks performed:
/// - Signature (ES256, with the resolved public key)
/// - Expiry (`exp`)
/// - Issuer (`iss` must match `cfg.issuer`)
/// - Token type must be `"access"`
///
/// Intentionally does **not** re-check the hardware attestation — that step
/// happened once at `/auth/exchange` on the validation worker and is encoded
/// in the signed token.
pub fn validate_jwt(token: &str, cfg: &JwtConfig) -> Result<GatewayClaims, JwtError> {
    let public_key = cfg
        .resolved_public_key()
        .map_err(|e| JwtError::PublicKeyResolution(e.to_string()))?;

    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_issuer(&[&cfg.issuer]);
    // exp is validated automatically by the `jsonwebtoken` crate.

    let TokenData { claims, .. } = decode::<GatewayClaims>(
        token,
        &DecodingKey::from_ec_pem(public_key.as_bytes())?,
        &validation,
    )?;

    if claims.token_type != "access" {
        return Err(JwtError::WrongTokenType);
    }

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

// ─────────────────────────────────────────────────────────────────────────────
// Unused constructor — kept as a reminder of the old interface
// ─────────────────────────────────────────────────────────────────────────────

/// Produce a `Header` for documentation purposes.
///
/// JWT *minting* now lives entirely in the Axum validation worker
/// (`validation_worker::auth::jwt`). The gateway only validates.
#[allow(dead_code)]
fn _hs256_header() -> Header {
    Header::new(Algorithm::HS256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JwtConfig;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    }

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

    const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAElCR0ys2PS/qszcku8bm+8JJ8wNLK\n\
IOY+VYNghIkMNmY2GtVnOKApnJ6p+q7oXcBOqnImEgwEb83cDdMNp1vIqg==\n\
-----END PUBLIC KEY-----";

    fn test_cfg() -> JwtConfig {
        unsafe {
            std::env::set_var("GATEWAY_PUBLIC_KEY", TEST_PUBLIC_KEY_PEM);
        }
        JwtConfig {
            issuer: "logger-gateway".to_string(),
            ttl_seconds: 900,
        }
    }

    fn make_test_token(cfg: &JwtConfig, token_type: &str) -> String {
        use jsonwebtoken::{encode, EncodingKey};
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let claims = GatewayClaims {
            sub: "example-app-prod".to_owned(),
            tid: "example_tenant".to_owned(),
            plt: "android".to_owned(),
            app: "com.test".to_owned(),
            device_uuid: "device-uuid-1".to_owned(),
            session_id: "session-id-1".to_owned(),
            token_type: token_type.to_owned(),
            exp: now + 9_999_999, // far future
            iat: now,
            iss: cfg.issuer.clone(),
        };
        encode(
            &Header::new(Algorithm::ES256),
            &claims,
            &EncodingKey::from_ec_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn access_token_validates_ok() {
        let _guard = TEST_LOCK.lock().unwrap();
        let cfg = test_cfg();
        let token = make_test_token(&cfg, "access");
        let claims = validate_jwt(&token, &cfg).unwrap();
        assert_eq!(claims.sub, "example-app-prod");
        assert_eq!(claims.tid, "example_tenant");
        assert_eq!(claims.device_uuid, "device-uuid-1");
        assert_eq!(claims.session_id, "session-id-1");
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn refresh_token_is_rejected() {
        let _guard = TEST_LOCK.lock().unwrap();
        let cfg = test_cfg();
        let token = make_test_token(&cfg, "refresh");
        assert!(matches!(
            validate_jwt(&token, &cfg),
            Err(JwtError::WrongTokenType)
        ));
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let _guard = TEST_LOCK.lock().unwrap();
        let cfg = test_cfg();
        let token = make_test_token(&cfg, "access");
        unsafe {
            std::env::set_var("GATEWAY_PUBLIC_KEY", "-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE14cM3v8/JkZ2oFpx7WqL/kX6Z7S9\n\
u8m2g9b/qszcku8bm+8JJ8wNLKIOY+VYNghIkMNmY2GtVnOKApnJ6p+q7oXcBOqn\n\
-----END PUBLIC KEY-----");
        }
        
        assert!(validate_jwt(&token, &cfg).is_err());
    }

    #[test]
    fn extract_bearer_ok() {
        assert_eq!(extract_bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer("Basic abc"), None);
        assert_eq!(extract_bearer(""), None);
    }

    #[test]
    fn validates_ok_with_raw_base64_public_key() {
        let _guard = TEST_LOCK.lock().unwrap();
        let raw_pub_key = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAElCR0ys2PS/qszcku8bm+8JJ8wNLKIOY+VYNghIkMNmY2GtVnOKApnJ6p+q7oXcBOqnImEgwEb83cDdMNp1vIqg==";
        unsafe {
            std::env::set_var("GATEWAY_PUBLIC_KEY", raw_pub_key);
        }
        let cfg = JwtConfig {
            issuer: "logger-gateway".to_string(),
            ttl_seconds: 900,
        };
        let token = make_test_token(&cfg, "access");
        let claims = validate_jwt(&token, &cfg).unwrap();
        assert_eq!(claims.sub, "example-app-prod");
    }
}
