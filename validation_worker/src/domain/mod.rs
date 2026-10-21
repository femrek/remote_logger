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
    /// Populated from client body (kept as-is from log entry).
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<String>,
    /// Populated from client body (kept as-is from log entry).
    #[serde(rename = "deviceId", alias = "deviceUUID", alias = "device_id", skip_serializing_if = "Option::is_none", default)]
    pub device_id: Option<String>,
    /// Stamped from JWT `sub` claim injected by gateway as `x-device-id`. Not expected from client body.
    #[serde(rename = "deviceIdJwt", skip_serializing_if = "Option::is_none", skip_deserializing, default)]
    pub device_id_jwt: Option<String>,
    /// Stamped from JWT `session_id` claim injected by gateway as `x-session-id`. Not expected from client body.
    #[serde(rename = "sessionIdJwt", skip_serializing_if = "Option::is_none", skip_deserializing, default)]
    pub session_id_jwt: Option<String>,
    pub error: Option<String>,
    pub stacktrace: Option<String>,
    /// Hardware attestation level: tee | software | none.
    /// Injected by logger_gateway from the validated JWT `att` claim via `x-attestation` header. Not expected from client body.
    #[serde(skip_serializing_if = "Option::is_none", skip_deserializing, default)]
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
