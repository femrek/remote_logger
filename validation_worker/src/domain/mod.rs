//! Domain model — pure data types shared across all layers.
//!
//! This module intentionally has **no** I/O or framework imports.
//! All types here are plain Rust structs / enums with serde derive.

use serde::{Deserialize, Serialize};

// ── Log record produced by the mobile client ──────────────────────────────────

/// Single log record produced by the mobile client.
///
/// Serde renames match the camelCase JSON keys the Dart client emits.
/// Per spec report §3, `session_id` is `Option<String>` because the Dart
/// client's `toJsonPayload()` never serialises the field.
#[derive(Debug, Deserialize, Serialize)]
pub struct ServerLog {
    pub timestamp: String,
    #[serde(rename = "loggerName")]
    pub logger_name: String,
    pub message: String,
    pub level: String,
    pub zone: String,
    /// Made `Option` intentionally — the mobile client omits this field.
    /// See spec report §3: "Missing sessionId Serialization on the Client".
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    /// Required by the OpenAPI spec and present in every mobile payload.
    #[serde(rename = "deviceUUID")]
    pub device_uuid: String,
    pub error: Option<String>,
    pub stacktrace: Option<String>,
}

// ── Gateway envelope ──────────────────────────────────────────────────────────

/// Wrapper that the Pingora gateway injects before forwarding to this worker.
///
/// Per spec report §2: sending a raw `ServerLog` without this envelope will
/// cause serde to reject the body and return 400 Bad Request.
#[derive(Debug, Deserialize)]
pub struct IngestEnvelope {
    pub app_id: String,
    pub tenant_id: String,
    pub payload: serde_json::Value,
}

// ── Kafka message ─────────────────────────────────────────────────────────────

/// Final message serialised to Kafka.
///
/// `#[serde(flatten)]` expands `ServerLog` fields directly into the root
/// object — there is **no** nested `payload` key in the Kafka message.
/// Per spec report §4, Fluent Bit's `nest/lift` filter is therefore a no-op.
#[derive(Debug, Serialize, Deserialize)]
pub struct KafkaPayload {
    pub tenant_id: String,
    pub app_id: String,
    pub log_index_prefix: String,
    #[serde(flatten)]
    pub log: ServerLog,
}
