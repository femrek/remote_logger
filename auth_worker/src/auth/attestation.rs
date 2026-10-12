//! Hardware attestation verifier trait and platform-specific implementations.

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

pub type VerifyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AttestationError>> + Send + 'a>>;

// ─────────────────────────────────────────────────────────────────────────────
// Pluggable verifier trait
// ─────────────────────────────────────────────────────────────────────────────

/// Object-safe, async attestation verifier.
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
