//! Hardware attestation verifier trait and platform-specific implementations.
//!
//! - **Android (`AndroidKeyAttestationVerifier`)**: Offline X.509 certificate chain
//!   and ASN.1 Key Attestation extension (`1.3.6.1.4.1.11129.2.1.17`) verification
//!   for keys generated in Android Hardware Keystore (`TEE` or `StrongBox`).
//!   Requires zero Google Cloud APIs or project numbers.
//! - **iOS (`AppleAppAttestVerifier`)**: Supports both Apple DeviceCheck App Attest
//!   CBOR attestation objects (`fmt = "apple-appattest"`) and Secure Enclave
//!   ECDSA P-256 challenge signature proofs.

use std::{future::Future, pin::Pin};

use base64::Engine;
use ring::signature::{
    UnparsedPublicKey, ECDSA_P256_SHA256_ASN1, ECDSA_P384_SHA256_ASN1, ECDSA_P384_SHA384_ASN1,
    RSA_PKCS1_2048_8192_SHA256, RSA_PKCS1_2048_8192_SHA384, RSA_PKCS1_2048_8192_SHA512,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Domain types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Ios,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Android => write!(f, "android"),
            Platform::Ios => write!(f, "ios"),
        }
    }
}

/// Verified hardware attestation security tier embedded in the JWT `att` claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationTier {
    StrongBox,
    Tee,
    SecureEnclave,
    None,
}

impl AttestationTier {
    pub fn as_str(self) -> &'static str {
        match self {
            AttestationTier::StrongBox => "strongbox",
            AttestationTier::Tee => "tee",
            AttestationTier::SecureEnclave => "secure_enclave",
            AttestationTier::None => "none",
        }
    }
}

impl std::fmt::Display for AttestationTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Input context for verifying a hardware attestation statement.
#[derive(Debug, Clone)]
pub struct AttestationContext<'a> {
    /// Base64-encoded attestation payload (Android X.509 PEM chain or iOS CBOR / DER signature).
    pub token: &'a str,
    /// Client mobile platform (`android` or `ios`).
    pub platform: Platform,
    /// Expected application package name (Android) or bundle ID (iOS).
    pub app_id: &'a str,
    /// Expected stateless challenge issued by `GET /auth/challenge`.
    pub challenge: Option<&'a str>,
    /// Client-enrolled Base64 P-256 public key (SPKI or SEC1).
    pub public_key: Option<&'a str>,
    /// Optional list of allowed Android APK signing certificate SHA-256 hex digests.
    /// When empty, APK signing certificate digest matching is skipped.
    pub android_signing_cert_sha256: &'a [String],
    /// Optional Apple Developer Team ID (10-char string) for iOS App Attest `rpIdHash` verification.
    pub apple_team_id: Option<&'a str>,
    /// Optional Apple App Attest environment (`"development"` or `"production"`).
    pub apple_app_attest_env: Option<&'a str>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AttestationError {
    #[error("Token is invalid or untrusted: {0}")]
    Invalid(String),
    #[error("Attestation vendor API error: {0}")]
    Upstream(String),
    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Object-safe boxed future & Pluggable verifier trait
// ─────────────────────────────────────────────────────────────────────────────

pub type VerifyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AttestationTier, AttestationError>> + Send + 'a>>;

/// Object-safe, async hardware attestation verifier.
pub trait AttestationVerifier: Send + Sync {
    fn verify<'a>(&'a self, ctx: AttestationContext<'a>) -> VerifyFuture<'a>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Noop verifier — used in tests / local mocks
// ─────────────────────────────────────────────────────────────────────────────

/// Always passes. Used for unit tests and mock environments.
pub struct NoopVerifier;

impl AttestationVerifier for NoopVerifier {
    fn verify<'a>(&'a self, ctx: AttestationContext<'a>) -> VerifyFuture<'a> {
        let tier = match ctx.platform {
            Platform::Android => AttestationTier::Tee,
            Platform::Ios => AttestationTier::SecureEnclave,
        };
        Box::pin(async move { Ok(tier) })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OIDs & Constants
// ─────────────────────────────────────────────────────────────────────────────

/// OID for Android Keystore Key Attestation v3 extension: `1.3.6.1.4.1.11129.2.1.17`
const ANDROID_KEY_ATTESTATION_OID_BYTES: &[u8] = &[
    0x2b, 0x06, 0x01, 0x04, 0x01, 0xd6, 0x79, 0x02, 0x01, 0x11,
];

/// OID for Apple App Attest Nonce extension: `1.2.840.113635.100.8.2`
const APPLE_APP_ATTEST_NONCE_OID_BYTES: &[u8] = &[
    0x2a, 0x86, 0x48, 0x86, 0xf7, 0x63, 0x64, 0x08, 0x02,
];

/// Signature algorithm OIDs (RFC 5280 / RFC 5758):
const OID_ECDSA_WITH_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
const OID_ECDSA_WITH_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x03];
const OID_RSA_WITH_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
const OID_RSA_WITH_SHA384: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0c];
const OID_RSA_WITH_SHA512: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0d];

const AAGUID_APPATTEST_DEVELOP: &[u8; 16] = b"appattestdevelop";
const AAGUID_APPATTEST_PROD: &[u8; 16] = b"appattest\0\0\0\0\0\0\0";

// ─────────────────────────────────────────────────────────────────────────────
// Android Hardware Keystore Attestation Verifier (TEE / StrongBox)
// ─────────────────────────────────────────────────────────────────────────────

/// Offline verifier for Android Hardware Keystore Key Attestation X.509 certificate chains.
#[derive(Default, Clone)]
pub struct AndroidKeyAttestationVerifier {
    /// Optional explicit trusted root certificate DER blobs.
    /// When empty, accepts self-signed hardware attestation roots (excluding Android Software Attestation roots).
    pub trusted_root_certs_der: Vec<Vec<u8>>,
}

impl AndroidKeyAttestationVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_trusted_roots(roots_der: Vec<Vec<u8>>) -> Self {
        Self {
            trusted_root_certs_der: roots_der,
        }
    }
}

/// Parsed fields from the Android Key Attestation ASN.1 extension (`1.3.6.1.4.1.11129.2.1.17`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAndroidAttestation {
    pub attestation_version: i64,
    pub attestation_security_level: i64,
    pub keymaster_security_level: i64,
    pub challenge: Vec<u8>,
    pub package_names: Vec<String>,
    /// Hex-encoded lowercase SHA-256 digests of the APK signing certificates.
    pub signature_digests_hex: Vec<String>,
}

impl AttestationVerifier for AndroidKeyAttestationVerifier {
    fn verify<'a>(&'a self, ctx: AttestationContext<'a>) -> VerifyFuture<'a> {
        Box::pin(async move {
            if ctx.platform != Platform::Android {
                return Err(AttestationError::UnsupportedPlatform(
                    ctx.platform.to_string(),
                ));
            }

            if ctx.token.trim().is_empty() {
                return Err(AttestationError::Invalid(
                    "Empty Android attestation certificate chain".to_owned(),
                ));
            }

            // 1. Decode outer Base64 into PEM text (or accept direct PEM text)
            let pem_text = if ctx.token.trim_start().starts_with("-----BEGIN") {
                ctx.token.to_owned()
            } else {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(ctx.token.trim())
                    .map_err(|e| {
                        AttestationError::Invalid(format!(
                            "Attestation token is not valid Base64: {e}"
                        ))
                    })?;
                String::from_utf8(bytes).map_err(|e| {
                    AttestationError::Invalid(format!(
                        "Decoded attestation chain is not valid UTF-8 PEM: {e}"
                    ))
                })?
            };

            // 2. Parse PEM certificate chain into DER blobs
            let certs_der = parse_pem_certificates(&pem_text).map_err(|e| {
                AttestationError::Invalid(format!("Failed to parse PEM certificate chain: {e}"))
            })?;

            if certs_der.len() < 2 {
                return Err(AttestationError::Invalid(format!(
                    "Android attestation certificate chain requires at least 2 certificates (leaf + root), got {}",
                    certs_der.len()
                )));
            }

            // 3. Parse X.509 structure & verify cryptographic signatures across the chain
            let mut parsed_certs = Vec::with_capacity(certs_der.len());
            for (idx, der) in certs_der.iter().enumerate() {
                let parsed = parse_x509_der(der).map_err(|e| {
                    AttestationError::Invalid(format!("Invalid X.509 DER at cert[{idx}]: {e}"))
                })?;
                parsed_certs.push(parsed);
            }

            for i in 0..(parsed_certs.len() - 1) {
                let child = &parsed_certs[i];
                let issuer = &parsed_certs[i + 1];
                verify_x509_signature(child, issuer).map_err(|e| {
                    AttestationError::Invalid(format!(
                        "Certificate chain signature verification failed at cert[{i}]: {e}"
                    ))
                })?;
            }

            // Verify root certificate is self-signed and not an untrusted software root
            let root_cert = parsed_certs.last().unwrap();
            let root_der = certs_der.last().unwrap();
            verify_x509_signature(root_cert, root_cert).map_err(|e| {
                AttestationError::Invalid(format!(
                    "Root certificate self-signature verification failed: {e}"
                ))
            })?;

            if contains_ascii_case_insensitive(root_cert.subject_raw, b"software attestation")
                || contains_ascii_case_insensitive(root_cert.subject_raw, b"android debug")
            {
                return Err(AttestationError::Invalid(
                    "Android Software Attestation root certificate is not trusted".to_owned(),
                ));
            }

            if !self.trusted_root_certs_der.is_empty()
                && !self
                    .trusted_root_certs_der
                    .iter()
                    .any(|trusted| trusted == root_der)
            {
                return Err(AttestationError::Invalid(
                    "Root certificate does not match configured trusted roots".to_owned(),
                ));
            }

            // 4. Verify leaf certificate public key matches enrolled `public_key` (if supplied)
            if let Some(expected_pub_b64) = ctx.public_key {
                if !expected_pub_b64.trim().is_empty() {
                    let expected_pub_raw = base64::engine::general_purpose::STANDARD
                        .decode(expected_pub_b64.trim())
                        .map_err(|e| {
                            AttestationError::Invalid(format!(
                                "Malformed Base64 public_key in request: {e}"
                            ))
                        })?;
                    let expected_sec1 =
                        crate::auth::jwt::extract_p256_sec1_bytes(&expected_pub_raw).map_err(
                            |e| {
                                AttestationError::Invalid(format!(
                                    "Invalid request public_key: {e}"
                                ))
                            },
                        )?;

                    if parsed_certs[0].public_key_bytes != expected_sec1 {
                        return Err(AttestationError::Invalid(
                            "Leaf certificate public key does not match request public_key"
                                .to_owned(),
                        ));
                    }
                }
            }

            // 5. Extract and parse Android Key Attestation extension (1.3.6.1.4.1.11129.2.1.17)
            let ext_payload = extract_x509_extension_value(
                &certs_der[0],
                ANDROID_KEY_ATTESTATION_OID_BYTES,
            )
            .ok_or_else(|| {
                AttestationError::Invalid(
                    "Leaf certificate is missing Android Key Attestation extension (1.3.6.1.4.1.11129.2.1.17)"
                        .to_owned(),
                )
            })?;

            let parsed = parse_android_key_description(&ext_payload).map_err(|e| {
                AttestationError::Invalid(format!(
                    "Failed to parse Android KeyDescription ASN.1 extension: {e}"
                ))
            })?;

            // 6. Verify hardware security level (1 = TrustedEnvironment / TEE, 2 = StrongBox)
            let tier = match parsed.attestation_security_level {
                1 => AttestationTier::Tee,
                2 => AttestationTier::StrongBox,
                other => {
                    return Err(AttestationError::Invalid(format!(
                        "Untrusted attestationSecurityLevel ({other}): must be TEE (1) or StrongBox (2)"
                    )));
                }
            };

            // 7. Verify attestationChallenge matches server-issued challenge
            if let Some(expected_challenge) = ctx.challenge {
                if parsed.challenge != expected_challenge.as_bytes() {
                    return Err(AttestationError::Invalid(
                        "Certificate attestationChallenge does not match server challenge"
                            .to_owned(),
                    ));
                }
            }

            // 8. Verify AttestationApplicationId package name matches channel.app_id
            if !parsed.package_names.is_empty()
                && !parsed.package_names.iter().any(|pkg| pkg == ctx.app_id)
            {
                return Err(AttestationError::Invalid(format!(
                    "Attested package names {:?} do not match channel app_id '{}'",
                    parsed.package_names, ctx.app_id
                )));
            }

            // 9. Optional: Verify APK signing certificate SHA-256 digest if configured on channel
            if !ctx.android_signing_cert_sha256.is_empty() {
                let normalized_allowed: Vec<String> = ctx
                    .android_signing_cert_sha256
                    .iter()
                    .map(|s| normalize_hex_sha256(s))
                    .filter(|s| !s.is_empty())
                    .collect();

                if !normalized_allowed.is_empty() {
                    let matched = parsed
                        .signature_digests_hex
                        .iter()
                        .any(|digest| normalized_allowed.contains(digest));

                    if !matched {
                        return Err(AttestationError::Invalid(format!(
                            "APK signing certificate SHA-256 {:?} is not in channel allowed list {:?}",
                            parsed.signature_digests_hex, normalized_allowed
                        )));
                    }
                }
            }

            tracing::info!(
                app_id = %ctx.app_id,
                tier = %tier,
                attestation_version = parsed.attestation_version,
                packages = ?parsed.package_names,
                "Android Hardware Keystore attestation verified"
            );

            Ok(tier)
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Apple App Attest & Secure Enclave Verifier
// ─────────────────────────────────────────────────────────────────────────────

pub struct AppleAppAttestVerifier;

impl AttestationVerifier for AppleAppAttestVerifier {
    fn verify<'a>(&'a self, ctx: AttestationContext<'a>) -> VerifyFuture<'a> {
        Box::pin(async move {
            if ctx.platform != Platform::Ios {
                return Err(AttestationError::UnsupportedPlatform(
                    ctx.platform.to_string(),
                ));
            }

            if ctx.token.trim().is_empty() {
                return Err(AttestationError::Invalid(
                    "Empty iOS attestation statement".to_owned(),
                ));
            }

            let raw_bytes = base64::engine::general_purpose::STANDARD
                .decode(ctx.token.trim())
                .map_err(|e| {
                    AttestationError::Invalid(format!(
                        "iOS attestation statement is not valid Base64: {e}"
                    ))
                })?;

            // Mode 1: Check if payload is an Apple App Attest CBOR object
            if let Some(cbor_att) = try_parse_apple_cbor_attestation(&raw_bytes) {
                verify_apple_cbor_attestation(&cbor_att, &ctx)?;
                tracing::info!(
                    app_id = %ctx.app_id,
                    "iOS Apple App Attest CBOR statement verified"
                );
                return Ok(AttestationTier::SecureEnclave);
            }

            // Mode 2: Secure Enclave P-256 signature fallback (when apple_team_id is not enforced)
            if let Some(team_id) = ctx.apple_team_id.filter(|s| !s.trim().is_empty()) {
                return Err(AttestationError::Invalid(format!(
                    "Channel requires Apple App Attest CBOR receipt (apple_team_id='{team_id}' is set), \
                     but client sent a non-CBOR attestation payload"
                )));
            }

            let challenge = ctx.challenge.ok_or_else(|| {
                AttestationError::Invalid(
                    "Challenge is required for iOS Secure Enclave signature verification"
                        .to_owned(),
                )
            })?;
            let pub_key_b64 = ctx.public_key.ok_or_else(|| {
                AttestationError::Invalid(
                    "public_key is required for iOS Secure Enclave signature verification"
                        .to_owned(),
                )
            })?;
            let pub_key_bytes = base64::engine::general_purpose::STANDARD
                .decode(pub_key_b64.trim())
                .map_err(|e| {
                    AttestationError::Invalid(format!("Invalid Base64 public_key: {e}"))
                })?;

            crate::auth::jwt::verify_p256_signature(
                &pub_key_bytes,
                challenge.as_bytes(),
                &raw_bytes,
            )
            .map_err(|e| {
                AttestationError::Invalid(format!(
                    "iOS Secure Enclave challenge signature verification failed: {e}"
                ))
            })?;

            tracing::info!(
                app_id = %ctx.app_id,
                "iOS Secure Enclave hardware key signature verified"
            );
            Ok(AttestationTier::SecureEnclave)
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform dispatcher — selects the right verifier by platform
// ─────────────────────────────────────────────────────────────────────────────

pub struct PlatformDispatchVerifier {
    pub android: Box<dyn AttestationVerifier>,
    pub ios: Box<dyn AttestationVerifier>,
}

impl AttestationVerifier for PlatformDispatchVerifier {
    fn verify<'a>(&'a self, ctx: AttestationContext<'a>) -> VerifyFuture<'a> {
        match ctx.platform {
            Platform::Android => self.android.verify(ctx),
            Platform::Ios => self.ios.verify(ctx),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure-Rust PEM, X.509 DER & ASN.1 Parsers
// ─────────────────────────────────────────────────────────────────────────────

pub fn normalize_hex_sha256(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .flat_map(char::to_lowercase)
        .collect()
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle_lower: &[u8]) -> bool {
    if needle_lower.is_empty() || haystack.len() < needle_lower.len() {
        return false;
    }
    haystack
        .windows(needle_lower.len())
        .any(|w| w.eq_ignore_ascii_case(needle_lower))
}

/// Parse one or more `-----BEGIN CERTIFICATE----- ... -----END CERTIFICATE-----` blocks into DER bytes.
pub fn parse_pem_certificates(pem_str: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut certs = Vec::new();
    let mut in_cert = false;
    let mut b64_buf = String::new();

    for line in pem_str.lines() {
        let trimmed = line.trim();
        if trimmed == "-----BEGIN CERTIFICATE-----" {
            in_cert = true;
            b64_buf.clear();
        } else if trimmed == "-----END CERTIFICATE-----" {
            if in_cert {
                let der = base64::engine::general_purpose::STANDARD
                    .decode(&b64_buf)
                    .map_err(|e| format!("Invalid Base64 in PEM certificate block: {e}"))?;
                certs.push(der);
                in_cert = false;
                b64_buf.clear();
            }
        } else if in_cert && !trimmed.is_empty() {
            b64_buf.push_str(trimmed);
        }
    }

    if in_cert {
        return Err("Unterminated -----BEGIN CERTIFICATE----- block".to_owned());
    }

    Ok(certs)
}

#[derive(Debug)]
struct ParsedX509<'a> {
    /// Complete DER TLV bytes of `tbsCertificate` (what the issuer signs).
    tbs_raw: &'a [u8],
    /// Raw bytes of `issuer` Name SEQUENCE.
    issuer_raw: &'a [u8],
    /// Raw bytes of `subject` Name SEQUENCE.
    subject_raw: &'a [u8],
    /// Raw public key bytes extracted from `subjectPublicKeyInfo.subjectPublicKey` BIT STRING.
    public_key_bytes: &'a [u8],
    /// Signature algorithm OID bytes from `Certificate.signatureAlgorithm`.
    signature_alg_oid: &'a [u8],
    /// Raw signature bytes from `Certificate.signatureValue` BIT STRING.
    signature_bytes: &'a [u8],
}

/// Read a single ASN.1 DER TLV element from `input`, returning `(tag, value_slice, full_tlv_slice, rest_slice)`.
fn read_der_tlv_full(input: &[u8]) -> Result<(u32, &[u8], &[u8], &[u8]), &'static str> {
    if input.is_empty() {
        return Err("Unexpected EOF reading DER tag");
    }
    let first = input[0];
    let mut idx = 1;

    // Decode tag (supports single-byte tags and high-tag-number form like [709])
    let mut tag: u32 = first as u32;
    if (first & 0x1f) == 0x1f {
        tag = 0;
        loop {
            if idx >= input.len() {
                return Err("Unexpected EOF in high-tag-number DER tag");
            }
            let b = input[idx];
            idx += 1;
            tag = (tag << 7) | ((b & 0x7f) as u32);
            if (b & 0x80) == 0 {
                break;
            }
        }
    }

    if idx >= input.len() {
        return Err("Unexpected EOF reading DER length");
    }
    let len_byte = input[idx];
    idx += 1;

    let len: usize = if (len_byte & 0x80) == 0 {
        len_byte as usize
    } else {
        let num_octets = (len_byte & 0x7f) as usize;
        if num_octets == 0 || num_octets > 4 || idx + num_octets > input.len() {
            return Err("Invalid or unsupported DER length octets");
        }
        let mut l = 0usize;
        for _ in 0..num_octets {
            l = (l << 8) | (input[idx] as usize);
            idx += 1;
        }
        l
    };

    if idx + len > input.len() {
        return Err("DER element length exceeds buffer");
    }

    Ok((
        tag,
        &input[idx..idx + len],
        &input[0..idx + len],
        &input[idx + len..],
    ))
}

fn read_der_tlv(input: &[u8]) -> Result<(u32, &[u8], &[u8]), &'static str> {
    let (tag, val, _tlv, rest) = read_der_tlv_full(input)?;
    Ok((tag, val, rest))
}

fn parse_x509_der(der: &[u8]) -> Result<ParsedX509<'_>, &'static str> {
    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }
    let (cert_tag, cert_body, _) = read_der_tlv(der)?;
    if cert_tag != 0x30 {
        return Err("X.509 Certificate is not a SEQUENCE");
    }

    let (tbs_tag, tbs_body, tbs_raw, rest) = read_der_tlv_full(cert_body)?;
    if tbs_tag != 0x30 {
        return Err("TBSCertificate is not a SEQUENCE");
    }

    let (sig_alg_tag, sig_alg_body, rest) = read_der_tlv(rest)?;
    if sig_alg_tag != 0x30 {
        return Err("signatureAlgorithm is not a SEQUENCE");
    }
    let (oid_tag, signature_alg_oid, _) = read_der_tlv(sig_alg_body)?;
    if oid_tag != 0x06 {
        return Err("Expected OID in signatureAlgorithm");
    }

    let (sig_val_tag, sig_val_bitstring, _) = read_der_tlv(rest)?;
    if sig_val_tag != 0x03 || sig_val_bitstring.is_empty() || sig_val_bitstring[0] != 0x00 {
        return Err("Invalid signatureValue BIT STRING");
    }
    let signature_bytes = &sig_val_bitstring[1..];

    // Parse TBSCertificate:
    // [0] EXPLICIT Version OPTIONAL, serialNumber, signature, issuer, validity, subject, subjectPublicKeyInfo
    let mut cursor = tbs_body;
    let (first_tag, _first_val, after_first) = read_der_tlv(cursor)?;
    if first_tag == 0xa0 {
        // v2/v3 explicit version present; next is serialNumber
        let (_serial_tag, _serial_val, after_serial) = read_der_tlv(after_first)?;
        cursor = after_serial;
    } else {
        // v1 default version; first element was serialNumber
        cursor = after_first;
    }

    // signature AlgorithmIdentifier
    let (_inner_sig_tag, _inner_sig_val, cursor) = read_der_tlv(cursor)?;
    // issuer Name
    let (_issuer_tag, _issuer_val, issuer_raw, cursor) = read_der_tlv_full(cursor)?;
    // validity
    let (_val_tag, _val_body, cursor) = read_der_tlv(cursor)?;
    // subject Name
    let (_subj_tag, _subj_val, subject_raw, cursor) = read_der_tlv_full(cursor)?;
    // subjectPublicKeyInfo ::= SEQUENCE { algorithm, subjectPublicKey BIT STRING }
    let (spki_tag, spki_body, _cursor) = read_der_tlv(cursor)?;
    if spki_tag != 0x30 {
        return Err("SubjectPublicKeyInfo is not a SEQUENCE");
    }
    let (_alg_tag, _alg_body, spki_rest) = read_der_tlv(spki_body)?;
    let (pk_tag, pk_bitstring, _) = read_der_tlv(spki_rest)?;
    if pk_tag != 0x03 || pk_bitstring.is_empty() || pk_bitstring[0] != 0x00 {
        return Err("Invalid subjectPublicKey BIT STRING");
    }
    let public_key_bytes = &pk_bitstring[1..];

    Ok(ParsedX509 {
        tbs_raw,
        issuer_raw,
        subject_raw,
        public_key_bytes,
        signature_alg_oid,
        signature_bytes,
    })
}

fn verify_x509_signature(child: &ParsedX509<'_>, issuer: &ParsedX509<'_>) -> Result<(), String> {
    if child.issuer_raw != issuer.subject_raw {
        return Err("Certificate issuer DN does not match parent subject DN".to_owned());
    }

    let alg: &'static dyn ring::signature::VerificationAlgorithm =
        match child.signature_alg_oid {
            OID_ECDSA_WITH_SHA256 => {
                if issuer.public_key_bytes.len() == 65 {
                    &ECDSA_P256_SHA256_ASN1
                } else if issuer.public_key_bytes.len() == 97 {
                    &ECDSA_P384_SHA256_ASN1
                } else {
                    return Err(format!(
                        "Unsupported EC public key length ({}) for ecdsa-with-SHA256",
                        issuer.public_key_bytes.len()
                    ));
                }
            }
            OID_ECDSA_WITH_SHA384 => &ECDSA_P384_SHA384_ASN1,
            OID_RSA_WITH_SHA256 => &RSA_PKCS1_2048_8192_SHA256,
            OID_RSA_WITH_SHA384 => &RSA_PKCS1_2048_8192_SHA384,
            OID_RSA_WITH_SHA512 => &RSA_PKCS1_2048_8192_SHA512,
            other => {
                return Err(format!(
                    "Unsupported X.509 signature algorithm OID: {}",
                    hex::encode(other)
                ));
            }
        };

    let peer_key = UnparsedPublicKey::new(alg, issuer.public_key_bytes);
    peer_key
        .verify(child.tbs_raw, child.signature_bytes)
        .map_err(|_| "Cryptographic signature verification failed".to_owned())
}

fn parse_der_integer(bytes: &[u8]) -> Result<i64, &'static str> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err("Invalid DER integer length");
    }
    let mut val: i64 = if (bytes[0] & 0x80) != 0 { -1 } else { 0 };
    for &b in bytes {
        val = (val << 8) | (b as i64);
    }
    Ok(val)
}

/// Locate an X.509 v3 extension by its raw OID bytes inside a DER-encoded certificate
/// and return the inner `extnValue` OCTET STRING payload.
pub fn extract_x509_extension_value(cert_der: &[u8], oid_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut oid_tlv = Vec::with_capacity(2 + oid_bytes.len());
    oid_tlv.push(0x06);
    oid_tlv.push(oid_bytes.len() as u8);
    oid_tlv.extend_from_slice(oid_bytes);

    for pos in 0..cert_der.len().saturating_sub(oid_tlv.len()) {
        if &cert_der[pos..pos + oid_tlv.len()] == oid_tlv.as_slice() {
            let mut cursor = &cert_der[pos + oid_tlv.len()..];
            // Optional critical BOOLEAN (tag 0x01)
            if let Ok((tag, _val, rest)) = read_der_tlv(cursor) {
                if tag == 0x01 {
                    cursor = rest;
                }
            }
            // Next element is the extnValue OCTET STRING (tag 0x04)
            if let Ok((0x04, octet_val, _)) = read_der_tlv(cursor) {
                return Some(octet_val.to_vec());
            }
        }
    }
    None
}

/// Parse the Android Keystore `KeyDescription` ASN.1 SEQUENCE from extension `1.3.6.1.4.1.11129.2.1.17`.
pub fn parse_android_key_description(
    ext_payload: &[u8],
) -> Result<ParsedAndroidAttestation, &'static str> {
    let (seq_tag, seq_body, _) = read_der_tlv(ext_payload)?;
    if seq_tag != 0x30 {
        return Err("KeyDescription is not an ASN.1 SEQUENCE");
    }

    // 0: attestationVersion (INTEGER, tag 0x02)
    let (t0, v0, rest) = read_der_tlv(seq_body)?;
    if t0 != 0x02 {
        return Err("Expected INTEGER for attestationVersion");
    }
    let attestation_version = parse_der_integer(v0)?;

    // 1: attestationSecurityLevel (ENUMERATED, tag 0x0A)
    let (t1, v1, rest) = read_der_tlv(rest)?;
    if t1 != 0x0a && t1 != 0x02 {
        return Err("Expected ENUMERATED for attestationSecurityLevel");
    }
    let attestation_security_level = parse_der_integer(v1)?;

    // 2: keymasterVersion (INTEGER, tag 0x02)
    let (t2, _v2, rest) = read_der_tlv(rest)?;
    if t2 != 0x02 {
        return Err("Expected INTEGER for keymasterVersion");
    }

    // 3: keymasterSecurityLevel (ENUMERATED, tag 0x0A)
    let (t3, v3, rest) = read_der_tlv(rest)?;
    if t3 != 0x0a && t3 != 0x02 {
        return Err("Expected ENUMERATED for keymasterSecurityLevel");
    }
    let keymaster_security_level = parse_der_integer(v3)?;

    // 4: attestationChallenge (OCTET STRING, tag 0x04)
    let (t4, challenge_bytes, rest) = read_der_tlv(rest)?;
    if t4 != 0x04 {
        return Err("Expected OCTET STRING for attestationChallenge");
    }

    // 5: uniqueId (OCTET STRING, tag 0x04)
    let (t5, _unique_id, rest) = read_der_tlv(rest)?;
    if t5 != 0x04 {
        return Err("Expected OCTET STRING for uniqueId");
    }

    // 6: softwareEnforced (AuthorizationList SEQUENCE, tag 0x30)
    let (t6, sw_enforced, rest) = read_der_tlv(rest)?;
    if t6 != 0x30 {
        return Err("Expected SEQUENCE for softwareEnforced");
    }

    // 7: teeEnforced (AuthorizationList SEQUENCE, tag 0x30)
    let (t7, tee_enforced, _) = read_der_tlv(rest)?;
    if t7 != 0x30 {
        return Err("Expected SEQUENCE for teeEnforced");
    }

    let mut package_names = Vec::new();
    let mut signature_digests_hex = Vec::new();

    for auth_list in [sw_enforced, tee_enforced] {
        if let Some((pkgs, sigs)) = extract_attestation_application_id(auth_list) {
            for p in pkgs {
                if !package_names.contains(&p) {
                    package_names.push(p);
                }
            }
            for s in sigs {
                if !signature_digests_hex.contains(&s) {
                    signature_digests_hex.push(s);
                }
            }
        }
    }

    Ok(ParsedAndroidAttestation {
        attestation_version,
        attestation_security_level,
        keymaster_security_level,
        challenge: challenge_bytes.to_vec(),
        package_names,
        signature_digests_hex,
    })
}

/// Scan an `AuthorizationList` SEQUENCE for tag `[709]` (`AttestationApplicationId`)
/// and parse its `package_infos` and `signature_digests`.
fn extract_attestation_application_id(auth_list: &[u8]) -> Option<(Vec<String>, Vec<String>)> {
    let mut cursor = auth_list;
    while !cursor.is_empty() {
        let (tag, val, rest) = read_der_tlv(cursor).ok()?;
        cursor = rest;
        if tag == 709 {
            let (inner_tag, app_id_der, _) = read_der_tlv(val).ok()?;
            let app_id_bytes = if inner_tag == 0x04 { app_id_der } else { val };
            return parse_attestation_application_id_der(app_id_bytes).ok();
        }
    }
    None
}

fn parse_attestation_application_id_der(
    der: &[u8],
) -> Result<(Vec<String>, Vec<String>), &'static str> {
    let (seq_tag, seq_body, _) = read_der_tlv(der)?;
    if seq_tag != 0x30 {
        return Err("AttestationApplicationId is not a SEQUENCE");
    }

    // 1. package_infos: SET OF AttestationPackageInfo (tag 0x31)
    let (set1_tag, mut pkg_set_cursor, rest) = read_der_tlv(seq_body)?;
    if set1_tag != 0x31 {
        return Err("Expected SET for package_infos");
    }

    let mut packages = Vec::new();
    while !pkg_set_cursor.is_empty() {
        let (p_tag, p_body, p_rest) = read_der_tlv(pkg_set_cursor)?;
        pkg_set_cursor = p_rest;
        if p_tag == 0x30 {
            let (name_tag, name_bytes, _) = read_der_tlv(p_body)?;
            if name_tag == 0x04 {
                if let Ok(pkg_str) = std::str::from_utf8(name_bytes) {
                    packages.push(pkg_str.to_owned());
                }
            }
        }
    }

    // 2. signature_digests: SET OF OCTET STRING (tag 0x31)
    let mut digests = Vec::new();
    if !rest.is_empty() {
        let (set2_tag, mut sig_set_cursor, _) = read_der_tlv(rest)?;
        if set2_tag == 0x31 {
            while !sig_set_cursor.is_empty() {
                let (d_tag, d_bytes, d_rest) = read_der_tlv(sig_set_cursor)?;
                sig_set_cursor = d_rest;
                if d_tag == 0x04 {
                    digests.push(hex::encode(d_bytes));
                }
            }
        }
    }

    Ok((packages, digests))
}

// ─────────────────────────────────────────────────────────────────────────────
// Apple App Attest CBOR Parser & Verifier
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct AppleCborAttestation {
    fmt: String,
    x5c: Vec<Vec<u8>>,
    auth_data: Vec<u8>,
}

/// Minimal RFC 8949 CBOR reader for Apple App Attest objects:
/// `{ "fmt": "apple-appattest", "attStmt": { "x5c": [cert0, cert1], "receipt": ... }, "authData": h'...' }`
fn try_parse_apple_cbor_attestation(bytes: &[u8]) -> Option<AppleCborAttestation> {
    let mut pos = 0;
    let map_len = read_cbor_header(bytes, &mut pos, 5)? as usize;

    let mut fmt = None;
    let mut x5c = Vec::new();
    let mut auth_data = None;

    for _ in 0..map_len {
        let key = read_cbor_text(bytes, &mut pos)?;
        match key.as_str() {
            "fmt" => {
                fmt = Some(read_cbor_text(bytes, &mut pos)?);
            }
            "authData" => {
                auth_data = Some(read_cbor_bytes(bytes, &mut pos)?);
            }
            "attStmt" => {
                let stmt_len = read_cbor_header(bytes, &mut pos, 5)? as usize;
                for _ in 0..stmt_len {
                    let stmt_key = read_cbor_text(bytes, &mut pos)?;
                    if stmt_key == "x5c" {
                        let arr_len = read_cbor_header(bytes, &mut pos, 4)? as usize;
                        for _ in 0..arr_len {
                            x5c.push(read_cbor_bytes(bytes, &mut pos)?);
                        }
                    } else {
                        skip_cbor_value(bytes, &mut pos)?;
                    }
                }
            }
            _ => {
                skip_cbor_value(bytes, &mut pos)?;
            }
        }
    }

    let fmt = fmt?;
    if fmt != "apple-appattest" {
        return None;
    }

    Some(AppleCborAttestation {
        fmt,
        x5c,
        auth_data: auth_data?,
    })
}

fn read_cbor_initial(bytes: &[u8], pos: &mut usize) -> Option<(u8, u64)> {
    if *pos >= bytes.len() {
        return None;
    }
    let b = bytes[*pos];
    *pos += 1;
    let major = b >> 5;
    let info = b & 0x1f;
    let val = match info {
        0..=23 => info as u64,
        24 => {
            if *pos + 1 > bytes.len() {
                return None;
            }
            let v = bytes[*pos] as u64;
            *pos += 1;
            v
        }
        25 => {
            if *pos + 2 > bytes.len() {
                return None;
            }
            let v = u16::from_be_bytes([bytes[*pos], bytes[*pos + 1]]) as u64;
            *pos += 2;
            v
        }
        26 => {
            if *pos + 4 > bytes.len() {
                return None;
            }
            let v = u32::from_be_bytes([
                bytes[*pos],
                bytes[*pos + 1],
                bytes[*pos + 2],
                bytes[*pos + 3],
            ]) as u64;
            *pos += 4;
            v
        }
        27 => {
            if *pos + 8 > bytes.len() {
                return None;
            }
            let v = u64::from_be_bytes([
                bytes[*pos],
                bytes[*pos + 1],
                bytes[*pos + 2],
                bytes[*pos + 3],
                bytes[*pos + 4],
                bytes[*pos + 5],
                bytes[*pos + 6],
                bytes[*pos + 7],
            ]);
            *pos += 8;
            v
        }
        _ => return None,
    };
    Some((major, val))
}

fn read_cbor_header(bytes: &[u8], pos: &mut usize, expected_major: u8) -> Option<u64> {
    let (major, val) = read_cbor_initial(bytes, pos)?;
    if major != expected_major {
        return None;
    }
    Some(val)
}

fn read_cbor_bytes(bytes: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let len = read_cbor_header(bytes, pos, 2)? as usize;
    if *pos + len > bytes.len() {
        return None;
    }
    let slice = bytes[*pos..*pos + len].to_vec();
    *pos += len;
    Some(slice)
}

fn read_cbor_text(bytes: &[u8], pos: &mut usize) -> Option<String> {
    let len = read_cbor_header(bytes, pos, 3)? as usize;
    if *pos + len > bytes.len() {
        return None;
    }
    let s = std::str::from_utf8(&bytes[*pos..*pos + len]).ok()?.to_owned();
    *pos += len;
    Some(s)
}

fn skip_cbor_value(bytes: &[u8], pos: &mut usize) -> Option<()> {
    let (major, val) = read_cbor_initial(bytes, pos)?;
    match major {
        0 | 1 | 7 => Some(()),
        2 | 3 => {
            let len = val as usize;
            if *pos + len > bytes.len() {
                return None;
            }
            *pos += len;
            Some(())
        }
        4 => {
            for _ in 0..val {
                skip_cbor_value(bytes, pos)?;
            }
            Some(())
        }
        5 => {
            for _ in 0..val {
                skip_cbor_value(bytes, pos)?;
                skip_cbor_value(bytes, pos)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn verify_apple_cbor_attestation(
    att: &AppleCborAttestation,
    ctx: &AttestationContext<'_>,
) -> Result<(), AttestationError> {
    if att.fmt != "apple-appattest" {
        return Err(AttestationError::Invalid(format!(
            "Unsupported Apple attestation format: {}",
            att.fmt
        )));
    }
    if att.x5c.is_empty() {
        return Err(AttestationError::Invalid(
            "Apple App Attest x5c certificate chain is empty".to_owned(),
        ));
    }

    // 1. Parse x5c certificates and verify signatures along chain
    let mut certs = Vec::with_capacity(att.x5c.len());
    for (idx, der) in att.x5c.iter().enumerate() {
        let cert = parse_x509_der(der).map_err(|e| {
            AttestationError::Invalid(format!("Invalid X.509 cert in Apple x5c[{idx}]: {e}"))
        })?;
        certs.push(cert);
    }

    for i in 0..certs.len().saturating_sub(1) {
        verify_x509_signature(&certs[i], &certs[i + 1]).map_err(|e| {
            AttestationError::Invalid(format!(
                "Apple x5c certificate signature failed at index {i}: {e}"
            ))
        })?;
    }

    // 2. Verify nonce extension (1.2.840.113635.100.8.2) in leaf cert (x5c[0])
    if let Some(challenge) = ctx.challenge {
        let client_data_hash = Sha256::digest(challenge.as_bytes());
        let mut nonce_input = Vec::with_capacity(att.auth_data.len() + 32);
        nonce_input.extend_from_slice(&att.auth_data);
        nonce_input.extend_from_slice(&client_data_hash);
        let expected_nonce = Sha256::digest(&nonce_input);

        let ext_val =
            extract_x509_extension_value(&att.x5c[0], APPLE_APP_ATTEST_NONCE_OID_BYTES)
                .ok_or_else(|| {
                    AttestationError::Invalid(
                        "Apple leaf certificate missing nonce extension (1.2.840.113635.100.8.2)"
                            .to_owned(),
                    )
                })?;

        let cert_nonce = extract_apple_nonce_from_ext(&ext_val).map_err(|e| {
            AttestationError::Invalid(format!("Failed to parse Apple nonce ASN.1 extension: {e}"))
        })?;

        if cert_nonce.as_slice() != expected_nonce.as_slice() {
            return Err(AttestationError::Invalid(
                "Apple App Attest certificate nonce does not match SHA256(authData || SHA256(challenge))"
                    .to_owned(),
            ));
        }
    }

    // 3. Verify authenticatorData (rpIdHash, signCount == 0, and optional aaguid)
    if att.auth_data.len() < 55 {
        return Err(AttestationError::Invalid(format!(
            "Apple authData too short ({} bytes, expected >= 55)",
            att.auth_data.len()
        )));
    }

    if let Some(team_id) = ctx.apple_team_id.filter(|s| !s.trim().is_empty()) {
        let app_id_full = format!("{}.{}", team_id.trim(), ctx.app_id);
        let expected_rp_hash = Sha256::digest(app_id_full.as_bytes());
        if &att.auth_data[0..32] != expected_rp_hash.as_slice() {
            return Err(AttestationError::Invalid(format!(
                "Apple App Attest rpIdHash mismatch for '{app_id_full}'"
            )));
        }
    }

    let sign_count = u32::from_be_bytes([
        att.auth_data[33],
        att.auth_data[34],
        att.auth_data[35],
        att.auth_data[36],
    ]);
    if sign_count != 0 {
        return Err(AttestationError::Invalid(format!(
            "Apple App Attest initial signCount must be 0, got {sign_count}"
        )));
    }

    if let Some(env) = ctx.apple_app_attest_env.filter(|s| !s.trim().is_empty()) {
        let aaguid = &att.auth_data[37..53];
        match env.trim().to_lowercase().as_str() {
            "development" if aaguid != AAGUID_APPATTEST_DEVELOP => {
                return Err(AttestationError::Invalid(
                    "Apple App Attest AAGUID does not match 'development' environment".to_owned(),
                ));
            }
            "production" if aaguid != AAGUID_APPATTEST_PROD => {
                return Err(AttestationError::Invalid(
                    "Apple App Attest AAGUID does not match 'production' environment".to_owned(),
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Extract the 32-byte nonce from Apple's `1.2.840.113635.100.8.2` extension value:
/// `SEQUENCE { [1] EXPLICIT OCTET STRING (32 bytes) }`.
fn extract_apple_nonce_from_ext(ext_val: &[u8]) -> Result<Vec<u8>, &'static str> {
    let (seq_tag, seq_body, _) = read_der_tlv(ext_val)?;
    if seq_tag != 0x30 {
        return Err("Expected SEQUENCE in Apple nonce extension");
    }
    let (_ctx_tag, ctx_body, _) = read_der_tlv(seq_body)?;
    let (oct_tag, nonce_bytes, _) = read_der_tlv(ctx_body)?;
    if oct_tag != 0x04 {
        return Err("Expected OCTET STRING inside Apple nonce extension");
    }
    Ok(nonce_bytes.to_vec())
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{
        rand::SystemRandom,
        signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
    };

    fn der_encode_tlv(tag: &[u8], content: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(tag);
        let len = content.len();
        if len < 128 {
            out.push(len as u8);
        } else if len <= 0xff {
            out.push(0x81);
            out.push(len as u8);
        } else {
            out.push(0x82);
            out.push((len >> 8) as u8);
            out.push((len & 0xff) as u8);
        }
        out.extend_from_slice(content);
        out
    }

    /// Build a synthetic DER-encoded `KeyDescription` extension payload for testing.
    fn build_test_key_description_der(
        security_level: u8,
        challenge: &[u8],
        package_name: &str,
        signing_cert_sha256: &[u8],
    ) -> Vec<u8> {
        let pkg_name_tlv = der_encode_tlv(&[0x04], package_name.as_bytes());
        let pkg_ver_tlv = der_encode_tlv(&[0x02], &[0x01]);
        let mut pkg_info_seq_body = Vec::new();
        pkg_info_seq_body.extend_from_slice(&pkg_name_tlv);
        pkg_info_seq_body.extend_from_slice(&pkg_ver_tlv);
        let pkg_info_seq = der_encode_tlv(&[0x30], &pkg_info_seq_body);
        let pkg_infos_set = der_encode_tlv(&[0x31], &pkg_info_seq);

        let sig_octet = der_encode_tlv(&[0x04], signing_cert_sha256);
        let sig_set = der_encode_tlv(&[0x31], &sig_octet);

        let mut app_id_seq_body = Vec::new();
        app_id_seq_body.extend_from_slice(&pkg_infos_set);
        app_id_seq_body.extend_from_slice(&sig_set);
        let app_id_seq = der_encode_tlv(&[0x30], &app_id_seq_body);

        // Wrap in OCTET STRING inside [709] EXPLICIT (tag bytes: 0xBF, 0x85, 0x45)
        let app_id_octet = der_encode_tlv(&[0x04], &app_id_seq);
        let tag_709 = der_encode_tlv(&[0xbf, 0x85, 0x45], &app_id_octet);

        let sw_enforced = der_encode_tlv(&[0x30], &tag_709);
        let tee_enforced = der_encode_tlv(&[0x30], &[]);

        let mut kd_body = Vec::new();
        kd_body.extend_from_slice(&der_encode_tlv(&[0x02], &[0x04])); // attestationVersion = 4
        kd_body.extend_from_slice(&der_encode_tlv(&[0x0a], &[security_level])); // attestationSecurityLevel
        kd_body.extend_from_slice(&der_encode_tlv(&[0x02], &[0x04])); // keymasterVersion = 4
        kd_body.extend_from_slice(&der_encode_tlv(&[0x0a], &[security_level])); // keymasterSecurityLevel
        kd_body.extend_from_slice(&der_encode_tlv(&[0x04], challenge)); // attestationChallenge
        kd_body.extend_from_slice(&der_encode_tlv(&[0x04], b"unique-id")); // uniqueId
        kd_body.extend_from_slice(&sw_enforced);
        kd_body.extend_from_slice(&tee_enforced);

        der_encode_tlv(&[0x30], &kd_body)
    }

    /// Build a minimal valid X.509 v3 DER certificate signed with `issuer_key`.
    fn build_x509_cert_der(
        serial: u8,
        issuer_cn: &str,
        subject_cn: &str,
        subject_pub_sec1: &[u8],
        extensions_der: Option<(&[u8], &[u8])>,
        issuer_key: &EcdsaKeyPair,
        rng: &SystemRandom,
    ) -> Vec<u8> {
        let version_tlv = der_encode_tlv(&[0xa0], &der_encode_tlv(&[0x02], &[0x02])); // v3
        let serial_tlv = der_encode_tlv(&[0x02], &[serial]);
        let sig_alg_tlv = der_encode_tlv(
            &[0x30],
            &der_encode_tlv(&[0x06], OID_ECDSA_WITH_SHA256),
        );

        let encode_cn = |cn: &str| -> Vec<u8> {
            let oid_cn = der_encode_tlv(&[0x06], &[0x55, 0x04, 0x03]);
            let val_cn = der_encode_tlv(&[0x0c], cn.as_bytes());
            let mut atv = Vec::new();
            atv.extend_from_slice(&oid_cn);
            atv.extend_from_slice(&val_cn);
            let rdn = der_encode_tlv(&[0x31], &der_encode_tlv(&[0x30], &atv));
            der_encode_tlv(&[0x30], &rdn)
        };

        let issuer_tlv = encode_cn(issuer_cn);
        let subject_tlv = encode_cn(subject_cn);

        let not_before = der_encode_tlv(&[0x17], b"250101000000Z");
        let not_after = der_encode_tlv(&[0x17], b"350101000000Z");
        let mut validity_body = Vec::new();
        validity_body.extend_from_slice(&not_before);
        validity_body.extend_from_slice(&not_after);
        let validity_tlv = der_encode_tlv(&[0x30], &validity_body);

        // SubjectPublicKeyInfo for P-256
        let mut spki = vec![
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06,
            0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
        ];
        spki.extend_from_slice(subject_pub_sec1);

        let mut tbs_body = Vec::new();
        tbs_body.extend_from_slice(&version_tlv);
        tbs_body.extend_from_slice(&serial_tlv);
        tbs_body.extend_from_slice(&sig_alg_tlv);
        tbs_body.extend_from_slice(&issuer_tlv);
        tbs_body.extend_from_slice(&validity_tlv);
        tbs_body.extend_from_slice(&subject_tlv);
        tbs_body.extend_from_slice(&spki);

        if let Some((ext_oid, ext_val)) = extensions_der {
            let mut ext_seq_body = Vec::new();
            ext_seq_body.extend_from_slice(&der_encode_tlv(&[0x06], ext_oid));
            ext_seq_body.extend_from_slice(&der_encode_tlv(&[0x04], ext_val));
            let ext_seq = der_encode_tlv(&[0x30], &ext_seq_body);
            let exts_seq = der_encode_tlv(&[0x30], &ext_seq);
            let exts_explicit = der_encode_tlv(&[0xa3], &exts_seq);
            tbs_body.extend_from_slice(&exts_explicit);
        }

        let tbs_tlv = der_encode_tlv(&[0x30], &tbs_body);
        let sig = issuer_key.sign(rng, &tbs_tlv).unwrap();

        let mut sig_bitstring_body = vec![0x00];
        sig_bitstring_body.extend_from_slice(sig.as_ref());
        let sig_bitstring = der_encode_tlv(&[0x03], &sig_bitstring_body);

        let mut cert_body = Vec::new();
        cert_body.extend_from_slice(&tbs_tlv);
        cert_body.extend_from_slice(&sig_alg_tlv);
        cert_body.extend_from_slice(&sig_bitstring);
        der_encode_tlv(&[0x30], &cert_body)
    }

    fn der_to_pem_cert(der: &[u8]) -> String {
        let b64 = base64::engine::general_purpose::STANDARD.encode(der);
        format!("-----BEGIN CERTIFICATE-----\n{b64}\n-----END CERTIFICATE-----\n")
    }

    fn generate_test_android_cert_chain(
        security_level: u8,
        challenge: &str,
        package_name: &str,
        signing_cert_sha256: &[u8],
    ) -> (String, String, Vec<u8>) {
        let rng = SystemRandom::new();
        let root_pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let root_key = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            root_pkcs8.as_ref(),
            &rng,
        )
        .unwrap();

        let leaf_pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let leaf_key = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            leaf_pkcs8.as_ref(),
            &rng,
        )
        .unwrap();

        let root_cn = "Google Hardware Attestation Root Test";
        let leaf_cn = "Android Keystore Key";

        let root_der = build_x509_cert_der(
            1,
            root_cn,
            root_cn,
            root_key.public_key().as_ref(),
            None,
            &root_key,
            &rng,
        );

        let kd_der = build_test_key_description_der(
            security_level,
            challenge.as_bytes(),
            package_name,
            signing_cert_sha256,
        );

        let leaf_der = build_x509_cert_der(
            2,
            root_cn,
            leaf_cn,
            leaf_key.public_key().as_ref(),
            Some((ANDROID_KEY_ATTESTATION_OID_BYTES, &kd_der)),
            &root_key,
            &rng,
        );

        let mut pem_chain = String::new();
        pem_chain.push_str(&der_to_pem_cert(&leaf_der));
        pem_chain.push_str(&der_to_pem_cert(&root_der));

        let token_b64 = base64::engine::general_purpose::STANDARD.encode(pem_chain.as_bytes());
        let leaf_pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(leaf_key.public_key().as_ref());

        (token_b64, leaf_pub_b64, root_der)
    }

    #[tokio::test]
    async fn android_verifier_accepts_valid_tee_and_strongbox_chains() {
        let cert_digest = [0xabu8; 32];
        let (token_b64, pub_b64, root_der) = generate_test_android_cert_chain(
            2, // StrongBox
            "server-challenge-123",
            "dev.femrek.remote_logging_example",
            &cert_digest,
        );

        let verifier = AndroidKeyAttestationVerifier::with_trusted_roots(vec![root_der]);

        // 1. With empty android_signing_cert_sha256 (optional mode) -> succeeds and returns StrongBox
        let tier = verifier
            .verify(AttestationContext {
                token: &token_b64,
                platform: Platform::Android,
                app_id: "dev.femrek.remote_logging_example",
                challenge: Some("server-challenge-123"),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &[],
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .expect("StrongBox chain should verify");
        assert_eq!(tier, AttestationTier::StrongBox);

        // 2. With matching android_signing_cert_sha256 (colon-separated uppercase) -> succeeds
        let allowed = vec![cert_digest
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")];
        let tier2 = verifier
            .verify(AttestationContext {
                token: &token_b64,
                platform: Platform::Android,
                app_id: "dev.femrek.remote_logging_example",
                challenge: Some("server-challenge-123"),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &allowed,
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .expect("Matching signing cert SHA-256 should verify");
        assert_eq!(tier2, AttestationTier::StrongBox);
    }

    #[tokio::test]
    async fn android_verifier_rejects_mismatched_challenge_or_app_id_or_software_level() {
        let cert_digest = [0x11u8; 32];
        let (token_b64, pub_b64, root_der) = generate_test_android_cert_chain(
            1, // TEE
            "expected-challenge",
            "dev.femrek.app",
            &cert_digest,
        );
        let verifier = AndroidKeyAttestationVerifier::with_trusted_roots(vec![root_der]);

        // Wrong challenge -> fails
        let err = verifier
            .verify(AttestationContext {
                token: &token_b64,
                platform: Platform::Android,
                app_id: "dev.femrek.app",
                challenge: Some("wrong-challenge"),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &[],
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("attestationChallenge"));

        // Wrong app_id -> fails
        let err = verifier
            .verify(AttestationContext {
                token: &token_b64,
                platform: Platform::Android,
                app_id: "com.attacker.repackaged",
                challenge: Some("expected-challenge"),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &[],
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("app_id"));

        // Wrong signing cert SHA-256 when configured -> fails
        let wrong_allowed = vec!["00".repeat(32)];
        let err = verifier
            .verify(AttestationContext {
                token: &token_b64,
                platform: Platform::Android,
                app_id: "dev.femrek.app",
                challenge: Some("expected-challenge"),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &wrong_allowed,
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("signing certificate SHA-256"));

        // Software security level (0) -> fails
        let (sw_token, sw_pub, sw_root) = generate_test_android_cert_chain(
            0, // Software
            "expected-challenge",
            "dev.femrek.app",
            &cert_digest,
        );
        let sw_verifier = AndroidKeyAttestationVerifier::with_trusted_roots(vec![sw_root]);
        let err = sw_verifier
            .verify(AttestationContext {
                token: &sw_token,
                platform: Platform::Android,
                app_id: "dev.femrek.app",
                challenge: Some("expected-challenge"),
                public_key: Some(&sw_pub),
                android_signing_cert_sha256: &[],
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("attestationSecurityLevel"));
    }

    #[tokio::test]
    async fn ios_verifier_accepts_secure_enclave_signature_when_team_id_omitted() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let key_pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                .unwrap();

        let challenge = "ios-challenge-nonce";
        let sig = key_pair.sign(&rng, challenge.as_bytes()).unwrap();
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.as_ref());
        let pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(key_pair.public_key().as_ref());

        let verifier = AppleAppAttestVerifier;
        let tier = verifier
            .verify(AttestationContext {
                token: &sig_b64,
                platform: Platform::Ios,
                app_id: "dev.femrek.exampleapp",
                challenge: Some(challenge),
                public_key: Some(&pub_b64),
                android_signing_cert_sha256: &[],
                apple_team_id: None,
                apple_app_attest_env: None,
            })
            .await
            .expect("Secure Enclave signature should verify");
        assert_eq!(tier, AttestationTier::SecureEnclave);
    }
}
