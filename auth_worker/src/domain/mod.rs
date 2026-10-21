//! Domain models for the Auth Worker.

use serde::{Deserialize, Serialize};

pub use crate::auth::attestation::Platform;

// ── Device metadata ───────────────────────────────────────────────────────────

/// Hardware and OS metadata collected by the mobile client at session start.
///
/// Sent once during `/auth/exchange` alongside the attestation token.
/// After validation, serialised as a `SystemStartupEvent` and produced to
/// the `device-metadata` Kafka topic — it is NOT embedded in the JWT or
/// forwarded on subsequent ingest requests.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeviceMetadata {
    pub timestamp: String,
    #[serde(rename = "deviceId", alias = "deviceUUID", alias = "device_id")]
    pub device_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub os: String,
    #[serde(rename = "osVersion")]
    pub os_version: String,
    pub model: String,
    pub brand: String,
    pub device: String,
    pub identifier: String,
    pub manufacturer: String,
    #[serde(rename = "isPhysicalDevice")]
    pub is_physical_device: String,
}

// ── Auth exchange request / response ─────────────────────────────────────────

// ── Auth challenge & enrollment ──────────────────────────────────────────────

/// Response body for `GET /auth/challenge`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthChallengeResponse {
    pub challenge: String,
    pub expires_in: u64,
}

/// Request body for `POST /auth/enroll`.
#[derive(Debug, Deserialize)]
pub struct AuthEnrollRequest {
    pub platform: Platform,
    pub device_id: String,
    /// Base64 public key (uncompressed SEC1 P-256 or SubjectPublicKeyInfo).
    pub public_key: String,
    /// Raw attestation blob from Keymaster cert chain (Android) or App Attest CBOR receipt (iOS).
    pub attestation: String,
    /// Stateless challenge string from GET /auth/challenge.
    pub challenge: String,
    /// Optional signature over the challenge to prove key possession at enrollment time.
    pub client_data_signature: Option<String>,
}

/// Response body for `POST /auth/enroll`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthEnrollResponse {
    pub enrollment_token: String,
    pub expires_in: u64,
}

// ── Auth exchange request / response ─────────────────────────────────────────

/// Request body for `POST /auth/exchange`.
///
/// Combines the hardware attestation token or 90-day enrollment token proof
/// with the device metadata so the mobile client makes a single round-trip for
/// both authentication and metadata ingestion.
#[derive(Debug, Deserialize)]
pub struct AuthExchangeRequest {
    /// Mobile platform — routes to the correct attestation verifier.
    pub platform: Platform,
    /// Raw attestation blob from Android Hardware Keystore or Apple App Attest.
    /// Optional when `enrollment_token` and `challenge_signature` are supplied.
    #[serde(default)]
    pub attestation_token: String,
    /// Client-generated device ID — embedded in JWT subject `sub`.
    #[serde(alias = "device_uuid")]
    pub device_id: String,
    /// Full device metadata — serialised to Kafka, NOT embedded in the JWT.
    pub device_metadata: DeviceMetadata,
    /// Base64-encoded DER public key.
    pub device_public_key: Option<String>,
    /// Long-lived 90-day server-signed enrollment token.
    pub enrollment_token: Option<String>,
    /// Stateless challenge string from GET /auth/challenge.
    pub challenge: Option<String>,
    /// Client ECDSA signature over the challenge string.
    pub challenge_signature: Option<String>,
}

/// Response body for `POST /auth/exchange`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthExchangeResponse {
    /// Session access token. Present this as
    /// `Authorization: Bearer <token>` on ingest requests.
    pub access_token: String,
    /// Remaining lifetime of the access token in seconds.
    pub access_expires_in: u64,
}

// ── Kafka messages ────────────────────────────────────────────────────────────

/// System Startup event produced to the `device-metadata` Kafka topic.
///
/// Emitted once per successful `/auth/exchange` after attestation passes.
/// Downstream consumers (e.g. Fluent Bit → OpenSearch) can build a device
/// registry from these events.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStartupEvent {
    /// Target OpenSearch index prefix (e.g. `mobile-devices-{app_id}`).
    pub log_index_prefix: String,
    /// Always `"system_startup"` — used as a discriminator by consumers.
    pub event_type: String,
    pub app_id: String,
    pub device_id: String,
    pub session_id: String,
    pub platform: String,
    pub metadata: DeviceMetadata,
    /// UTC ISO-8601 timestamp of when this event was ingested by the worker.
    pub ingested_at: String,
}
