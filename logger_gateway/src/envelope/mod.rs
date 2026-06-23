use serde::Deserialize;
use serde_json::value::RawValue;
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("Payload size {received} bytes exceeds limit of {limit} bytes")]
    TooLarge { limit: usize, received: usize },

    #[error("JSON syntax error: {0}")]
    Parse(#[from] serde_json::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Envelope struct
// ─────────────────────────────────────────────────────────────────────────────

/// Shallow envelope extracted from every ingest request body.
///
/// ## Design rationale — the "opaque blob" invariant
///
/// `payload` is typed as `&'a RawValue` which means:
///
/// 1. **Syntax validation only** — `serde_json` verifies that the value at
///    `payload` is syntactically valid JSON (object, array, scalar, …) but
///    does not allocate or traverse its contents.
///
/// 2. **Zero deep allocation** — the `payload` pointer borrows directly from
///    the original `body` slice. No per-field `String` or `Vec` is created for
///    the inner log structure.
///
/// 3. **Schema independence** — mobile teams can add, remove, or rename fields
///    inside `payload` without a gateway redeploy. The gateway never breaks on
///    unknown inner fields.
///
/// 4. **Forwarding** — the raw body bytes are forwarded to the Kafka-writer
///    unchanged; no re-serialization cost.
///
/// ## Lifetime
/// `'a` is tied to the input `body` slice. The caller (proxy handler) owns
/// the `Bytes` buffer for the duration of the request.
#[derive(Debug, Deserialize)]
pub struct LogEnvelope<'a> {
    /// Application bundle identifier (e.g. `"com.acme.fancyapp"`).
    /// Used for per-app metrics and Kafka partition keys by the backend.
    pub app_id: String,

    /// Tenant / organization identifier.
    /// Must match the `tid` claim in the bearer JWT.
    pub tenant_id: String,

    /// Opaque inner log payload — never deeply validated by the gateway.
    /// `serde_json` confirms it is syntactically valid JSON; the gateway
    /// forwards the raw bytes verbatim to the Kafka-writer service.
    /// Note: this field is populated by serde, not by named-field access.
    #[allow(dead_code)]
    #[serde(borrow)]
    pub payload: &'a RawValue,
}

// ─────────────────────────────────────────────────────────────────────────────
// Parse
// ─────────────────────────────────────────────────────────────────────────────

/// Parse `body` into a [`LogEnvelope`].
///
/// Validates (in order):
/// 1. `body.len() <= max_bytes` — rejects oversized payloads early.
/// 2. Top-level JSON is syntactically valid.
/// 3. Required fields `app_id`, `tenant_id`, `payload` are present.
///
/// The inner `payload` value is **not** validated; it is treated as an opaque
/// blob and forwarded verbatim.
///
/// # Lifetime
/// The returned `LogEnvelope<'_>` borrows from `body`. Callers must ensure
/// `body` outlives the envelope.
pub fn parse_envelope(body: &[u8], max_bytes: usize) -> Result<LogEnvelope<'_>, EnvelopeError> {
    if body.len() > max_bytes {
        return Err(EnvelopeError::TooLarge {
            limit: max_bytes,
            received: body.len(),
        });
    }
    Ok(serde_json::from_slice(body)?)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 2 * 1024 * 1024;

    #[test]
    fn valid_envelope_parses() {
        let body = br#"{
            "app_id": "com.acme.app",
            "tenant_id": "acme",
            "payload": {"level": "info", "msg": "hello", "ts": 1234567890}
        }"#;
        let env = parse_envelope(body, MAX).unwrap();
        assert_eq!(env.app_id, "com.acme.app");
        assert_eq!(env.tenant_id, "acme");
        // payload is valid JSON but not traversed
        assert!(env.payload.get().contains("info"));
    }

    #[test]
    fn oversized_body_is_rejected() {
        let body = vec![b'x'; MAX + 1];
        let err = parse_envelope(&body, MAX).unwrap_err();
        assert!(matches!(err, EnvelopeError::TooLarge { .. }));
    }

    #[test]
    fn missing_field_is_rejected() {
        // `tenant_id` is absent
        let body = br#"{"app_id": "x", "payload": {}}"#;
        assert!(parse_envelope(body, MAX).is_err());
    }

    #[test]
    fn malformed_json_is_rejected() {
        let body = b"{not valid json}";
        assert!(parse_envelope(body, MAX).is_err());
    }

    #[test]
    fn complex_inner_payload_is_opaque() {
        // The inner payload has deeply nested / arbitrary structure — should parse fine.
        let body = br#"{
            "app_id": "com.test",
            "tenant_id": "test",
            "payload": {
                "events": [{"type": "click", "unknown_new_field": true}],
                "meta": {"sdk_version": "3.1.4", "new_key": [1,2,3]}
            }
        }"#;
        assert!(parse_envelope(body, MAX).is_ok());
    }
}
