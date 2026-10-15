//! Domain model for Validation Worker — log ingestion data types.

use serde::{Deserialize, Serialize};

// ── Log record produced by the mobile client ──────────────────────────────────

/// Single log record produced by the mobile client.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerLog {
    pub timestamp: String,
    #[serde(rename = "loggerName")]
    pub logger_name: String,
    pub message: String,
    pub level: String,
    pub zone: String,
    /// Populated from x-session-id (JWT claims) or client body.
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
    /// Populated from x-device-id (JWT claims) or client body.
    #[serde(rename = "deviceId", alias = "deviceUUID", alias = "device_id", default)]
    pub device_id: String,
    pub error: Option<String>,
    pub stacktrace: Option<String>,
    /// Hardware attestation level: tee | software | none.
    /// Injected by logger_gateway from the validated JWT `att` claim via `x-attestation` header.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attestation: Option<String>,
}

// ── Kafka messages ────────────────────────────────────────────────────────────

/// Final log message serialised to the `mobile-logs` Kafka topic.
#[derive(Debug, Serialize, Deserialize)]
pub struct KafkaPayload {
    pub app_id: String,
    pub log_index_prefix: String,
    #[serde(flatten)]
    pub log: ServerLog,
}
