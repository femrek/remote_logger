//! JWT minting for the auth worker.

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

pub struct JwtConfig<'a> {
    pub private_key: &'a str,
    pub access_ttl_seconds: u64,
    pub refresh_ttl_seconds: u64,
    pub issuer: &'a str,
}

// ─────────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerClaims {
    /// Subject — the channel `id` (e.g., `"example-app-prod"`).
    pub sub: String,
    /// Mobile platform the token was issued for (`"android"` | `"ios"`).
    pub plt: String,
    /// Application bundle ID.
    pub app: String,
    /// Device UUID provided by the client.
    pub device_uuid: String,
    /// Session ID generated server-side (UUID v4).
    pub session_id: String,
    /// `"access"` or `"refresh"`.
    pub token_type: String,
    /// SHA-256 hash (hex) of the iOS device public key — only present in
    /// refresh tokens when the channel has `require_apple_key_hash: true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apk_hash: Option<String>,
    /// Expiry (Unix timestamp seconds).
    pub exp: usize,
    /// Issued-at (Unix timestamp seconds).
    pub iat: usize,
    /// Issuer — must match the gateway's `jwt.issuer` config value.
    pub iss: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("JWT encoding failed: {0}")]
    Encoding(#[from] jsonwebtoken::errors::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Minting — access token
// ─────────────────────────────────────────────────────────────────────────────

pub fn issue_access_jwt(
    channel_id: &str,
    platform: &str,
    app_id: &str,
    device_uuid: &str,
    session_id: &str,
    cfg: &JwtConfig<'_>,
) -> Result<String, JwtError> {
    let now = Utc::now().timestamp() as usize;
    let claims = WorkerClaims {
        sub: channel_id.to_owned(),
        plt: platform.to_owned(),
        app: app_id.to_owned(),
        device_uuid: device_uuid.to_owned(),
        session_id: session_id.to_owned(),
        token_type: "access".to_owned(),
        apk_hash: None,
        exp: now + cfg.access_ttl_seconds as usize,
        iat: now,
        iss: cfg.issuer.to_owned(),
    };
    let encoding_key = EncodingKey::from_ec_pem(cfg.private_key.as_bytes())?;
    encode(
        &Header::new(Algorithm::ES256),
        &claims,
        &encoding_key,
    )
    .map_err(JwtError::Encoding)
}

// ─────────────────────────────────────────────────────────────────────────────
// Minting — refresh token
// ─────────────────────────────────────────────────────────────────────────────

pub fn issue_refresh_jwt(
    channel_id: &str,
    platform: &str,
    app_id: &str,
    device_uuid: &str,
    session_id: &str,
    apk_hash: Option<String>,
    cfg: &JwtConfig<'_>,
) -> Result<String, JwtError> {
    let now = Utc::now().timestamp() as usize;
    let claims = WorkerClaims {
        sub: channel_id.to_owned(),
        plt: platform.to_owned(),
        app: app_id.to_owned(),
        device_uuid: device_uuid.to_owned(),
        session_id: session_id.to_owned(),
        token_type: "refresh".to_owned(),
        apk_hash,
        exp: now + cfg.refresh_ttl_seconds as usize,
        iat: now,
        iss: cfg.issuer.to_owned(),
    };
    let encoding_key = EncodingKey::from_ec_pem(cfg.private_key.as_bytes())?;
    encode(
        &Header::new(Algorithm::ES256),
        &claims,
        &encoding_key,
    )
    .map_err(JwtError::Encoding)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/xzSZ8vFrWHzZjcl\n\
zdNiDxBcAFRguq8C6zYQ55IRjRehRANCAASUJHTKzY9L+qzNyS7xub7wknzA0sog\n\
5j5Vg2CEiQw2ZjYa1Wc4oCmcnqn6ruhdwE6qciYSDARvzdwN0w2nW8iq\n\
-----END PRIVATE KEY-----";

    const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAElCR0ys2PS/qszcku8bm+8JJ8wNLK\n\
IOY+VYNghIkMNmY2GtVnOKApnJ6p+q7oXcBOqnImEgwEb83cDdMNp1vIqg==\n\
-----END PUBLIC KEY-----";

    fn test_cfg() -> JwtConfig<'static> {
        JwtConfig {
            private_key: TEST_PRIVATE_KEY_PEM,
            access_ttl_seconds: 900,
            refresh_ttl_seconds: 604_800,
            issuer: "logger-gateway",
        }
    }

    #[test]
    fn access_token_has_correct_type() {
        let cfg = test_cfg();
        let token =
            issue_access_jwt("ch-1", "android", "app-1", "dev-uuid", "sess-uuid", &cfg)
                .unwrap();

        let mut v = Validation::new(Algorithm::ES256);
        v.set_issuer(&["logger-gateway"]);
        let data = decode::<WorkerClaims>(
            &token,
            &DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap(),
            &v,
        )
        .unwrap();
        assert_eq!(data.claims.token_type, "access");
        assert!(data.claims.apk_hash.is_none());
    }

    #[test]
    fn refresh_token_embeds_apk_hash() {
        let cfg = test_cfg();
        let hash = sha256_hex(b"fake-public-key-bytes");
        let token = issue_refresh_jwt(
            "ch-1",
            "ios",
            "app-1",
            "dev-uuid",
            "sess-uuid",
            Some(hash.clone()),
            &cfg,
        )
        .unwrap();

        let mut v = Validation::new(Algorithm::ES256);
        v.set_issuer(&["logger-gateway"]);
        let data = decode::<WorkerClaims>(
            &token,
            &DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap(),
            &v,
        )
        .unwrap();
        assert_eq!(data.claims.token_type, "refresh");
        assert_eq!(data.claims.apk_hash.as_deref(), Some(hash.as_str()));
    }

    #[test]
    fn refresh_token_without_apk_hash() {
        let cfg = test_cfg();
        let token =
            issue_refresh_jwt("ch-1", "android", "a", "d", "s", None, &cfg).unwrap();

        let mut v = Validation::new(Algorithm::ES256);
        v.set_issuer(&["logger-gateway"]);
        let data = decode::<WorkerClaims>(
            &token,
            &DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap(),
            &v,
        )
        .unwrap();
        assert!(data.claims.apk_hash.is_none());
    }

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, sha256_hex(b"world"));
    }
}
