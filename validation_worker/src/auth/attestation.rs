//! Hardware attestation verifier trait and platform-specific implementations.
//!
//! Migrated from `logger_gateway::auth::attestation` and extended with
//! the `device_public_key` parameter needed for stateless Apple refresh tokens.
//!
//! ## Production implementation checklist
//!
//! ### Google Play Integrity
//! 1. Authenticate with a Google Cloud service account (`yup-oauth2` or `google-authz`).
//! 2. POST the token to:
//!    `https://playintegrity.googleapis.com/v1/{package_name}:decodeIntegrityToken`
//! 3. Verify `requestDetails.requestPackageName` matches your app.
//! 4. Check `appIntegrity.appRecognitionVerdict == "PLAY_RECOGNIZED"`.
//! 5. Evaluate `deviceIntegrity.deviceRecognitionVerdict` against your risk threshold.
//! 6. Optionally validate the nonce against a server-generated challenge (replay protection).
//!
//! ### Apple App Attest
//! 1. Decode the CBOR-encoded attestation object (use the `ciborium` crate).
//! 2. Verify the X.509 certificate chain up to Apple's root CA
//!    (`Apple App Attestation Root CA`).
//! 3. Verify `SHA-256(clientDataHash || nonce)` matches the cert extension.
//! 4. Extract the `credCert.publicKey` (this is what `device_public_key` represents).
//! 5. For subsequent assertion requests, use `DCAppAttestService.generateAssertion`.

use std::{future::Future, pin::Pin};
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
// Object-safe boxed future
// ─────────────────────────────────────────────────────────────────────────────

/// Object-safe pinned future returned by [`AttestationVerifier::verify`].
///
/// Mirroring the `KafkaSink` pattern: using a manually-boxed future avoids the
/// `async_trait` macro while keeping the trait dyn-compatible so tests can
/// inject `NoopVerifier` via `Arc<dyn AttestationVerifier>`.
pub type VerifyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AttestationError>> + Send + 'a>>;

// ─────────────────────────────────────────────────────────────────────────────
// Pluggable verifier trait
// ─────────────────────────────────────────────────────────────────────────────

/// Object-safe, async attestation verifier.
///
/// Returns a boxed future so the trait is dyn-compatible and can be stored
/// behind `Arc<dyn AttestationVerifier>`.
pub trait AttestationVerifier: Send + Sync {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        platform: Platform,
        app_id: &'a str,
        google_project_number: Option<&'a str>,
        apple_team_id: Option<&'a str>,
    ) -> VerifyFuture<'a>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Noop verifier — used when validate_attestation = false
// ─────────────────────────────────────────────────────────────────────────────

/// Always passes. Used for dev channels / CI environments.
pub struct NoopVerifier;

impl AttestationVerifier for NoopVerifier {
    fn verify<'a>(
        &'a self,
        _: &'a str,
        _: Platform,
        _: &'a str,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> VerifyFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Google Play Integrity verifier (production stub)
// ─────────────────────────────────────────────────────────────────────────────

pub struct GooglePlayIntegrityVerifier;

impl AttestationVerifier for GooglePlayIntegrityVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        platform: Platform,
        app_id: &'a str,
        google_project_number: Option<&'a str>,
        _apple_team_id: Option<&'a str>,
    ) -> VerifyFuture<'a> {
        Box::pin(async move {
            if platform != Platform::Android {
                return Err(AttestationError::UnsupportedPlatform(platform.to_string()));
            }

            let project_number = google_project_number.ok_or_else(|| {
                AttestationError::Invalid(
                    "Missing google_project_number in channel configuration".to_owned(),
                )
            })?;

            // TODO: replace stub with real Google Play Integrity API call.
            tracing::warn!(
                token_prefix = &token[..token.len().min(12)],
                project_number = %project_number,
                package = %app_id,
                "⚠️  Google Play Integrity stub — ALWAYS PASSES. Implement before production."
            );
            Ok(())
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Apple App Attest verifier (production stub)
// ─────────────────────────────────────────────────────────────────────────────

pub struct AppleAppAttestVerifier;

impl AttestationVerifier for AppleAppAttestVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        platform: Platform,
        app_id: &'a str,
        _google_project_number: Option<&'a str>,
        apple_team_id: Option<&'a str>,
    ) -> VerifyFuture<'a> {
        Box::pin(async move {
            if platform != Platform::Ios {
                return Err(AttestationError::UnsupportedPlatform(platform.to_string()));
            }

            let team_id = apple_team_id.ok_or_else(|| {
                AttestationError::Invalid(
                    "Missing apple_team_id in channel configuration".to_owned(),
                )
            })?;

            // TODO: replace stub with real CBOR decode + certificate chain verification.
            tracing::warn!(
                token_prefix = &token[..token.len().min(12)],
                team_id = %team_id,
                bundle_id = %app_id,
                "⚠️  Apple App Attest stub — ALWAYS PASSES. Implement before production."
            );
            Ok(())
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform dispatcher — selects the right verifier by platform
// ─────────────────────────────────────────────────────────────────────────────

/// Routes attestation calls to the correct platform verifier.
/// Construct once at startup and share via `Arc`.
pub struct PlatformDispatchVerifier {
    pub android: Box<dyn AttestationVerifier>,
    pub ios: Box<dyn AttestationVerifier>,
}

impl AttestationVerifier for PlatformDispatchVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        platform: Platform,
        app_id: &'a str,
        google_project_number: Option<&'a str>,
        apple_team_id: Option<&'a str>,
    ) -> VerifyFuture<'a> {
        match platform {
            Platform::Android => self.android.verify(
                token,
                platform,
                app_id,
                google_project_number,
                apple_team_id,
            ),
            Platform::Ios => self.ios.verify(
                token,
                platform,
                app_id,
                google_project_number,
                apple_team_id,
            ),
        }
    }
}
