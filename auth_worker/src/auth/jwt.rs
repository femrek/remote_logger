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
    pub issuer: &'a str,
}

// ─────────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerClaims {
    /// Subject — the device ID (`device_id`).
    pub sub: String,
    /// Mobile platform the token was issued for (`"android"` | `"ios"`).
    pub plt: String,
    /// Application bundle ID.
    pub app: String,
    /// Session ID generated on client or server (UUID).
    pub session_id: String,
    /// Token type — always `"access"`.
    pub token_type: String,
    /// Hardware attestation level: `"strongbox"`, `"tee"`, `"secure_enclave"`, or `"none"`.
    #[serde(default = "default_att")]
    pub att: String,
    /// Expiry (Unix timestamp seconds).
    pub exp: usize,
    /// Issued-at (Unix timestamp seconds).
    pub iat: usize,
    /// Issuer — must match the gateway's `jwt.issuer` config value.
    pub iss: String,
}

fn default_att() -> String {
    "none".to_string()
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
    platform: &str,
    app_id: &str,
    device_id: &str,
    session_id: &str,
    att: &str,
    cfg: &JwtConfig<'_>,
) -> Result<String, JwtError> {
    let now = Utc::now().timestamp() as usize;
    let claims = WorkerClaims {
        sub: device_id.to_owned(),
        plt: platform.to_owned(),
        app: app_id.to_owned(),
        session_id: session_id.to_owned(),
        token_type: "access".to_owned(),
        att: att.to_owned(),
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
// Enrollment tokens & challenge proof
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentClaims {
    pub sub: String,
    pub plt: String,
    pub app: String,
    pub pub_key: String,
    pub att: String,
    pub token_type: String,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,
}

pub fn issue_enrollment_jwt(
    platform: &str,
    app_id: &str,
    device_id: &str,
    public_key_base64: &str,
    att: &str,
    ttl_seconds: u64,
    cfg: &JwtConfig<'_>,
) -> Result<String, JwtError> {
    let now = Utc::now().timestamp() as usize;
    let claims = EnrollmentClaims {
        sub: device_id.to_owned(),
        plt: platform.to_owned(),
        app: app_id.to_owned(),
        pub_key: public_key_base64.to_owned(),
        att: att.to_owned(),
        token_type: "enrollment".to_owned(),
        exp: now + ttl_seconds as usize,
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

pub fn decode_enrollment_jwt(
    token: &str,
    public_key_pem: &str,
    issuer: &str,
) -> Result<EnrollmentClaims, jsonwebtoken::errors::Error> {
    let key = jsonwebtoken::DecodingKey::from_ec_pem(public_key_pem.as_bytes())?;
    let mut validation = jsonwebtoken::Validation::new(Algorithm::ES256);
    validation.set_issuer(&[issuer]);
    let token_data = jsonwebtoken::decode::<EnrollmentClaims>(token, &key, &validation)?;
    Ok(token_data.claims)
}

pub fn create_challenge(private_key_pem: &str, ttl_seconds: u64) -> String {
    use sha2::{Digest, Sha256};
    let nonce = uuid::Uuid::new_v4().to_string();
    let exp = Utc::now().timestamp() + ttl_seconds as i64;
    let sig_input = format!("{}:{}:{}", nonce, exp, private_key_pem);
    let sig = hex::encode(Sha256::digest(sig_input.as_bytes()));
    format!("{}.{}.{}", nonce, exp, sig)
}

pub fn verify_challenge(challenge: &str, private_key_pem: &str) -> Result<(), &'static str> {
    use sha2::{Digest, Sha256};
    let parts: Vec<&str> = challenge.split('.').collect();
    if parts.len() != 3 {
        return Err("Malformed challenge format");
    }
    let nonce = parts[0];
    let exp: i64 = parts[1].parse().map_err(|_| "Invalid challenge expiration")?;
    let sig = parts[2];

    if Utc::now().timestamp() > exp {
        return Err("Challenge expired");
    }

    let sig_input = format!("{}:{}:{}", nonce, exp, private_key_pem);
    let expected_sig = hex::encode(Sha256::digest(sig_input.as_bytes()));
    if sig != expected_sig {
        return Err("Challenge signature mismatch");
    }
    Ok(())
}

pub fn derive_public_key_pem(private_key_pem: &str) -> Result<String, String> {
    use base64::Engine;
    use ring::signature::KeyPair;

    let lines: Vec<&str> = private_key_pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let b64 = lines.concat();
    let der = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("base64 decode: {e}"))?;

    let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        &der,
        &ring::rand::SystemRandom::new(),
    )
    .map_err(|e| format!("pkcs8 parse: {e}"))?;

    let pub_bytes = key_pair.public_key().as_ref();
    let mut spki = vec![
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
        0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
    ];
    spki.extend_from_slice(pub_bytes);
    let pub_b64 = base64::engine::general_purpose::STANDARD.encode(&spki);
    Ok(format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----", pub_b64))
}

pub fn extract_p256_sec1_bytes(raw: &[u8]) -> Result<&[u8], &'static str> {
    if raw.len() == 65 && raw[0] == 0x04 {
        Ok(raw)
    } else if raw.len() == 91 && raw[26] == 0x04 {
        Ok(&raw[26..])
    } else {
        Err("Unsupported public key format, expected SEC1 (65 bytes) or SPKI (91 bytes)")
    }
}

pub fn verify_p256_signature(
    public_key_bytes: &[u8],
    message: &[u8],
    signature_bytes: &[u8],
) -> Result<(), &'static str> {
    use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1, ECDSA_P256_SHA256_FIXED};

    let key_bytes = extract_p256_sec1_bytes(public_key_bytes)?;

    // Try ASN.1 DER (standard Keystore output)
    let peer_asn1 = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, key_bytes);
    if peer_asn1.verify(message, signature_bytes).is_ok() {
        return Ok(());
    }

    // Try raw 64-byte (r, s) fixed (standard Apple App Attest output)
    let peer_fixed = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, key_bytes);
    if peer_fixed.verify(message, signature_bytes).is_ok() {
        return Ok(());
    }

    Err("Invalid signature")
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
            access_ttl_seconds: 86_400,
            issuer: "logger-gateway",
        }
    }

    #[test]
    fn access_token_has_correct_type_and_att() {
        let cfg = test_cfg();
        let token =
            issue_access_jwt("android", "app-1", "dev-id", "sess-uuid", "strongbox", &cfg)
                .unwrap();

        let mut v = Validation::new(Algorithm::ES256);
        v.set_issuer(&["logger-gateway"]);
        let data = decode::<WorkerClaims>(
            &token,
            &DecodingKey::from_ec_pem(TEST_PUBLIC_KEY_PEM.as_bytes()).unwrap(),
            &v,
        )
        .unwrap();
        assert_eq!(data.claims.sub, "dev-id");
        assert_eq!(data.claims.token_type, "access");
        assert_eq!(data.claims.att, "strongbox");
        assert_eq!(data.claims.session_id, "sess-uuid");
    }

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, sha256_hex(b"world"));
    }

    #[test]
    fn challenge_creation_and_verification_works() {
        let challenge = create_challenge(TEST_PRIVATE_KEY_PEM, 300);
        assert!(verify_challenge(&challenge, TEST_PRIVATE_KEY_PEM).is_ok());
        assert!(verify_challenge(&challenge, "wrong-key").is_err());
        assert!(verify_challenge("malformed", TEST_PRIVATE_KEY_PEM).is_err());
    }

    #[test]
    fn enrollment_token_roundtrip() {
        let cfg = test_cfg();
        let token = issue_enrollment_jwt(
            "android",
            "app-1",
            "device-1",
            "fake-pubkey-base64",
            "tee",
            7776000,
            &cfg,
        )
        .unwrap();

        let claims = decode_enrollment_jwt(&token, TEST_PUBLIC_KEY_PEM, "logger-gateway").unwrap();
        assert_eq!(claims.sub, "device-1");
        assert_eq!(claims.app, "app-1");
        assert_eq!(claims.pub_key, "fake-pubkey-base64");
        assert_eq!(claims.att, "tee");
        assert_eq!(claims.token_type, "enrollment");
    }

    #[test]
    fn derive_public_key_pem_matches_expected() {
        let derived = derive_public_key_pem(TEST_PRIVATE_KEY_PEM).unwrap();
        let norm_derived: String = derived.lines().filter(|l| !l.starts_with("-----")).collect();
        let norm_expected: String = TEST_PUBLIC_KEY_PEM.lines().filter(|l| !l.starts_with("-----")).collect();
        assert_eq!(norm_derived, norm_expected);
        assert!(DecodingKey::from_ec_pem(derived.as_bytes()).is_ok());
    }

    #[test]
    fn verify_p256_signature_works() {
        use base64::Engine;
        use ring::signature::KeyPair;
        let lines: Vec<&str> = TEST_PRIVATE_KEY_PEM
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with("-----"))
            .collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(lines.concat())
            .unwrap();
        let rng = ring::rand::SystemRandom::new();
        let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &der,
            &rng,
        )
        .unwrap();

        let message = b"session-proof-challenge";
        let sig = key_pair.sign(&rng, message).unwrap();
        let pub_bytes = key_pair.public_key().as_ref();

        // 1. Verify with SEC1 uncompressed (65 bytes)
        assert!(verify_p256_signature(pub_bytes, message, sig.as_ref()).is_ok());

        // 2. Verify with SPKI (91 bytes)
        let mut spki = vec![
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
            0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
        ];
        spki.extend_from_slice(pub_bytes);
        assert!(verify_p256_signature(&spki, message, sig.as_ref()).is_ok());

        // 3. Verify mismatch
        assert!(verify_p256_signature(pub_bytes, b"wrong-message", sig.as_ref()).is_err());
    }
}
