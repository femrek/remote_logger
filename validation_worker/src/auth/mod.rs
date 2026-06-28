//! Authentication sub-modules for the validation worker.
//!
//! - `attestation`: hardware attestation verifier trait + platform stubs.
//! - `jwt`: access and refresh JWT minting.

pub mod attestation;
pub mod jwt;

pub use attestation::{
    AttestationError, AttestationVerifier, NoopVerifier,
    GooglePlayIntegrityVerifier, AppleAppAttestVerifier,
    PlatformDispatchVerifier, Platform,
};
pub use jwt::{WorkerClaims, JwtConfig, JwtError, issue_access_jwt, issue_refresh_jwt};
