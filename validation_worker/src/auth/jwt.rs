//! JWT minting for the validation worker.
//!
//! The worker mints two token types at `/auth/exchange`:
//! - **Access token** — short-lived (default 15 min), accepted by the gateway
//!   on ingest routes.
//! - **Refresh token** — long-lived (default 7 days), used to obtain new access
//!   tokens without re-running hardware attestation. For iOS channels with
//!   `require_apple_key_hash: true`, the refresh token embeds
//!   `hex(sha256(device_public_key))` in the `apk_hash` claim so any worker
//!   replica can statelessly verify the device's identity on refresh.
//!
//! Both tokens are signed with **HS256** using the `GATEWAY_JWT_SECRET` env var —
//! the same secret the Pingora gateway uses to validate access tokens on ingest
//! routes.

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

/// JWT configuration resolved from the `AppState` (originally from env vars).
pub struct JwtConfig<'a> {
    pub private_key: &'a str,
    pub access_ttl_seconds: u64,
    pub refresh_ttl_seconds: u64,
    pub issuer: &'a str,
}

// ─────────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────────

/// Claims embedded in every worker-issued JWT (both access and refresh).
///
/// The gateway's `GatewayClaims` struct must mirror the shape of this struct
/// for `validate_jwt` to succeed on ingest routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerClaims {
    /// Subject — the channel `id` (e.g., `"example-app-prod"`).
    pub sub: String,
    /// Tenant identifier.
    pub tid: String,
    /// Mobile platform the token was issued for (`"android"` | `"ios"`).
    pub plt: String,
    /// Application bundle ID.
    pub app: String,
    /// Device UUID provided by the client.
    pub device_uuid: String,
    /// Session ID generated server-side (UUID v4).
    pub session_id: String,
    /// `"access"` or `"refresh"`. The gateway rejects non-access tokens on
    /// ingest routes.
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

/// Mint a short-lived ES256 **access** token.
///
/// The gateway's `validate_jwt` checks that `token_type == "access"`, so
/// refresh tokens are automatically rejected on ingest routes.
pub fn issue_access_jwt(
    channel_id: &str,
    tenant_id: &str,
    platform: &str,
    app_id: &str,
    device_uuid: &str,
    session_id: &str,
    cfg: &JwtConfig<'_>,
) -> Result<String, JwtError> {
    let now = Utc::now().timestamp() as usize;
    let claims = WorkerClaims {
        sub: channel_id.to_owned(),
        tid: tenant_id.to_owned(),
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

/// Mint a long-lived ES256 **refresh** token.
///
/// When `apk_hash` is `Some`, the hash is embedded in the JWT claims so any
/// worker replica can statelessly verify that subsequent refresh requests come
/// from the same iOS device (by re-hashing the presented public key and
/// comparing). When `None`, the refresh token is opaque with respect to the
/// device key.
pub fn issue_refresh_jwt(
    channel_id: &str,
    tenant_id: &str,
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
        tid: tenant_id.to_owned(),
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

/// Compute `hex(sha256(bytes))` — used to hash the Apple device public key.
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
            issue_access_jwt("ch-1", "tenant-1", "android", "app-1", "dev-uuid", "sess-uuid", &cfg)
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
            "tenant-1",
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
            issue_refresh_jwt("ch-1", "t", "android", "a", "d", "s", None, &cfg).unwrap();

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
