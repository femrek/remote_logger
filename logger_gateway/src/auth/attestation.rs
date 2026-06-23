use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Domain types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

/// Request body sent to `/auth/exchange`.
#[derive(Debug, Deserialize)]
pub struct AttestationRequest {
    /// Mobile platform — used to route to the correct verifier.
    pub platform: Platform,
    /// Raw attestation token from Google Play Integrity or Apple App Attest.
    pub attestation_token: String,
    /// Tenant identifier claimed by the client (embedded in the issued JWT).
    pub tenant_id: String,
}

/// These are library-facing error variants; production verifier implementations
/// will return them. The `#[allow]` suppresses the PoC-level dead_code warning.
#[allow(dead_code)]
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
// Pluggable verifier trait
// ─────────────────────────────────────────────────────────────────────────────

/// Asynchronous attestation verifier.
///
/// Implement this trait to swap between:
/// - Real Google Play Integrity API calls
/// - Real Apple App Attest verification
/// - A `NoopVerifier` (for channels with `validate_attestation: false`)
/// - A `MockVerifier` in unit / integration tests
#[async_trait]
pub trait AttestationVerifier: Send + Sync {
    async fn verify(
        &self,
        token: &str,
        platform: Platform,
        tenant_id: &str,
        app_id: &str,
        google_project_number: Option<&str>,
        apple_team_id: Option<&str>,
    ) -> Result<(), AttestationError>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Noop verifier — used when validate_attestation = false
// ─────────────────────────────────────────────────────────────────────────────

/// Always passes. Used for dev channels / CI environments.
/// Suppressed: this is intentionally a library type, used when `validate_attestation = false`.
#[allow(dead_code)]
pub struct NoopVerifier;

#[async_trait]
impl AttestationVerifier for NoopVerifier {
    async fn verify(
        &self,
        _: &str,
        _: Platform,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<(), AttestationError> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Google Play Integrity verifier (production stub)
// ─────────────────────────────────────────────────────────────────────────────

/// Verifies Google Play Integrity tokens.
///
/// ## Production implementation checklist
/// 1. Authenticate with a Google Cloud service account (via `google-authz` or `yup-oauth2`).
/// 2. POST the token to:
///    `https://playintegrity.googleapis.com/v1/{package_name}:decodeIntegrityToken`
/// 3. Verify `requestDetails.requestPackageName` matches your app.
/// 4. Check `appIntegrity.appRecognitionVerdict == "PLAY_RECOGNIZED"`.
/// 5. Evaluate `deviceIntegrity.deviceRecognitionVerdict` against your risk threshold.
/// 6. Optionally validate the nonce against a server-generated challenge to prevent replay.
pub struct GooglePlayIntegrityVerifier;

#[async_trait]
impl AttestationVerifier for GooglePlayIntegrityVerifier {
    async fn verify(
        &self,
        token: &str,
        platform: Platform,
        _tenant_id: &str,
        app_id: &str,
        google_project_number: Option<&str>,
        _apple_team_id: Option<&str>,
    ) -> Result<(), AttestationError> {
        if platform != Platform::Android {
            return Err(AttestationError::UnsupportedPlatform(platform.to_string()));
        }

        let project_number = google_project_number.ok_or_else(|| {
            AttestationError::Invalid("Missing google_project_number in channel configuration".to_owned())
        })?;

        // TODO: replace stub with real API call.
        tracing::warn!(
            token_prefix = &token[..token.len().min(12)],
            project_number = %project_number,
            package = %app_id,
            "⚠️  Google Play Integrity stub — ALWAYS PASSES. Implement before production."
        );
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Apple App Attest verifier (production stub)
// ─────────────────────────────────────────────────────────────────────────────

/// Verifies Apple App Attest attestation objects.
///
/// ## Production implementation checklist
/// 1. Decode the CBOR-encoded attestation object (use the `ciborium` crate).
/// 2. Verify the X.509 certificate chain up to Apple's root CA
///    (`Apple App Attestation Root CA`).
/// 3. Verify the SHA-256 of the nonce matches the embedded nonce in the cert extension.
/// 4. Extract the `credCert.publicKey` and persist as the device's key identifier.
/// 5. For subsequent assertion requests, use `DCAppAttestService.generateAssertion`.
pub struct AppleAppAttestVerifier;

#[async_trait]
impl AttestationVerifier for AppleAppAttestVerifier {
    async fn verify(
        &self,
        token: &str,
        platform: Platform,
        _tenant_id: &str,
        app_id: &str,
        _google_project_number: Option<&str>,
        apple_team_id: Option<&str>,
    ) -> Result<(), AttestationError> {
        if platform != Platform::Ios {
            return Err(AttestationError::UnsupportedPlatform(platform.to_string()));
        }

        let team_id = apple_team_id.ok_or_else(|| {
            AttestationError::Invalid("Missing apple_team_id in channel configuration".to_owned())
        })?;

        // TODO: replace stub with real CBOR decode + certificate chain verification.
        tracing::warn!(
            token_prefix = &token[..token.len().min(12)],
            team_id = %team_id,
            bundle_id = %app_id,
            "⚠️  Apple App Attest stub — ALWAYS PASSES. Implement before production."
        );
        Ok(())
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

#[async_trait]
impl AttestationVerifier for PlatformDispatchVerifier {
    async fn verify(
        &self,
        token: &str,
        platform: Platform,
        tenant_id: &str,
        app_id: &str,
        google_project_number: Option<&str>,
        apple_team_id: Option<&str>,
    ) -> Result<(), AttestationError> {
        match platform {
            Platform::Android => {
                self.android
                    .verify(
                        token,
                        platform,
                        tenant_id,
                        app_id,
                        google_project_number,
                        apple_team_id,
                    )
                    .await
            }
            Platform::Ios => {
                self.ios
                    .verify(
                        token,
                        platform,
                        tenant_id,
                        app_id,
                        google_project_number,
                        apple_team_id,
                    )
                    .await
            }
        }
    }
}
