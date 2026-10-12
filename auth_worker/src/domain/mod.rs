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
    #[serde(rename = "deviceUUID")]
    pub device_uuid: String,
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

/// Request body for `POST /auth/exchange`.
///
/// Combines the hardware attestation token with the device metadata so the
/// mobile client makes a single round-trip for both authentication and
/// metadata ingestion.
#[derive(Debug, Deserialize)]
pub struct AuthExchangeRequest {
    /// Mobile platform — routes to the correct attestation verifier.
    pub platform: Platform,
    /// Raw attestation blob from Google Play Integrity or Apple App Attest.
    pub attestation_token: String,
    /// Client-generated device UUID — embedded in both JWTs.
    pub device_uuid: String,
    /// Full device metadata — serialised to Kafka, NOT embedded in the JWT.
    pub device_metadata: DeviceMetadata,
    /// Base64-encoded DER public key from Apple's `DCAppAttestService`.
    /// Required when the channel has `require_apple_key_hash: true` and
    /// `platform == ios`. Ignored otherwise.
    pub device_public_key: Option<String>,
}

/// Response body for `POST /auth/exchange`.
#[derive(Debug, Serialize)]
pub struct AuthExchangeResponse {
    /// Short-lived access token (default 15 min). Present this as
    /// `Authorization: Bearer <token>` on ingest requests.
    pub access_token: String,
    /// Long-lived refresh token (default 7 days). Use to obtain a new
    /// access token without repeating hardware attestation.
    pub refresh_token: String,
    /// Remaining lifetime of the access token in seconds.
    pub access_expires_in: u64,
    /// Remaining lifetime of the refresh token in seconds.
    pub refresh_expires_in: u64,
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
    pub device_uuid: String,
    pub session_id: String,
    pub platform: String,
    pub metadata: DeviceMetadata,
    /// UTC ISO-8601 timestamp of when this event was ingested by the worker.
    pub ingested_at: String,
}
