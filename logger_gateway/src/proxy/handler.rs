//! # Pingora ProxyHttp lifecycle implementation
//!
//! ## Request routing overview
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────┐
//! │  request_filter()                                                      │
//! │    ├─ path == "/auth/exchange"                                         │
//! │    │    └─ return Ok(false)  [PIPELINE CONTINUES → proxied to worker]  │
//! │    │                                                                   │
//! │    └─ path starts_with channel.path_prefix                             │
//! │         ├─ Validate Authorization: Bearer <jwt> (token_type="access")  │
//! │         ├─ Stash tenant_id + app_id in ctx                             │
//! │         └─ return Ok(false)  [PIPELINE CONTINUES]                      │
//! │                                                                        │
//! │  upstream_peer()                                                       │
//! │    └─ Round-robin select HttpPeer from the shared load balancer        │
//! │                                                                        │
//! │  upstream_request_filter()                                             │
//! │    ├─ /auth/exchange path → forward as-is (no header mutation)         │
//! │    └─ ingest path → strip Authorization; inject x-tenant-id + x-app-id│
//! │                                                                        │
//! │  request_body_filter()  [called per chunk, streaming from client]      │
//! │    ├─ Accumulate byte count (size guard only — body is opaque)         │
//! │    └─ Reject if buffer > max_body_bytes                                │
//! │                                                                        │
//! │  response_filter()                                                     │
//! │    └─ If ctx.reject → override upstream response status                │
//! │                                                                        │
//! │  response_body_filter()                                                │
//! │    └─ If ctx.reject → replace upstream body with our JSON error        │
//! │                                                                        │
//! │  logging()                                                             │
//! │    └─ Emit Prometheus metrics (latency, status, body size)             │
//! └────────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use pingora::http::{RequestHeader, ResponseHeader, StatusCode};
use pingora::proxy::{ProxyHttp, Session};
use pingora::upstreams::peer::HttpPeer;

use crate::auth::{extract_bearer, validate_jwt};
use crate::config::SharedConfig;
use crate::metrics::{
    BODY_SIZE_BYTES, ENVELOPE_ERRORS_TOTAL, JWT_VALIDATIONS_TOTAL,
    REQUEST_DURATION_SECONDS, REQUESTS_TOTAL,
};
use crate::discovery::ServiceDiscovery;

// ─────────────────────────────────────────────────────────────────────────────
// Per-request context
// ─────────────────────────────────────────────────────────────────────────────

/// State that flows through the Pingora lifecycle for a single request.
pub struct RequestContext {
    /// Channel id resolved in `request_filter`. Empty string before resolution.
    pub channel_id: String,
    /// Tenant id from JWT claims (ingest path).
    pub tenant_id: String,
    /// Application bundle identifier (e.g., `"dev.femrek.exampleapp"`).
    pub app_id: Option<String>,

    // ── Request classification ───────────────────────────────────────────────
    /// `true` when the path is `/auth/exchange` — headers must NOT be mutated.
    pub is_auth_exchange: bool,

    // ── Body buffering ───────────────────────────────────────────────────────
    /// Running total of bytes read (we no longer buffer the full body).
    pub body_bytes_read: usize,
    /// Copied from config at request start to avoid repeated load() calls.
    pub max_body_bytes: usize,

    // ── Error signalling between body filter and response filter ─────────────
    /// When `Some`, the body validation failed. `response_filter` and
    /// `response_body_filter` use this to override the upstream response with
    /// our structured error.
    pub reject: Option<(u16, &'static str)>,

    // ── Observability ────────────────────────────────────────────────────────
    pub start: Instant,
}

impl Default for RequestContext {
    fn default() -> Self {
        Self {
            channel_id: String::new(),
            tenant_id: String::new(),
            app_id: None,
            is_auth_exchange: false,
            body_bytes_read: 0,
            max_body_bytes: 2 * 1024 * 1024,
            reject: None,
            start: Instant::now(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Gateway struct
// ─────────────────────────────────────────────────────────────────────────────

pub struct LogGateway {
    /// Live routing table — updated in-place by the hot-reload watcher via
    /// `ArcSwap::store`. Workers read with `config.load()` on every request.
    pub config: SharedConfig,
    /// Dynamic load balancer for upstream selection.
    pub discovery: Arc<ServiceDiscovery>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Write a structured JSON error response to the downstream client and signal
/// that the pipeline should stop (returns `Ok(true)`).
async fn send_error(
    session: &mut Session,
    status: u16,
    error_code: &str,
    message: &str,
) -> pingora::Result<bool> {
    let body = format!(
        r#"{{"error":"{}","code":"{}"}}"#,
        message.replace('"', "\\\""),
        error_code
    );
    let body_bytes = Bytes::from(body.clone());

    let mut resp =
        ResponseHeader::build(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), None)?;
    resp.insert_header("content-type", "application/json")?;
    resp.insert_header("content-length", body_bytes.len().to_string())?;
    resp.insert_header("x-gateway", "logger-gateway/0.1")?;

    session
        .write_response_header(Box::new(resp), false)
        .await?;
    session
        .write_response_body(Some(body_bytes), true)
        .await?;

    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// ProxyHttp trait
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ProxyHttp for LogGateway {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        RequestContext::default()
    }

    // ── Phase 1: request_filter ───────────────────────────────────────────────
    //
    // Called after request headers are received, before the body is read or
    // the upstream connection is established.
    //
    // Return Ok(true)  → pipeline stops; response already written to client.
    // Return Ok(false) → pipeline continues; proceed to upstream_peer().
    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        let cfg = self.config.load();
        ctx.max_body_bytes = cfg.global.max_body_bytes;

        let path = session.req_header().uri.path().to_owned();

        // ── /auth/exchange ────────────────────────────────────────────────────
        // Blind proxy — the validation worker owns all attestation logic and
        // JWT minting. The gateway has nothing to do here except let it through.
        if path == "/auth/exchange" {
            ctx.channel_id = "auth".to_string();
            ctx.is_auth_exchange = true;
            return Ok(false); // pipeline continues → upstream_peer → worker
        }

        // ── /v1/logs/<channel> — ingest ───────────────────────────────────────
        let ch_cfg = match cfg.find_channel_by_path(&path) {
            Some(ch) => ch.clone(),
            None => {
                return send_error(session, 404, "CHANNEL_NOT_FOUND", "No channel for this path").await;
            }
        };

        ctx.channel_id = ch_cfg.id.clone();

        // Extract and validate the bearer JWT --------------------------------
        let auth_header = session
            .req_header()
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let token = match extract_bearer(auth_header) {
            Some(t) if !t.is_empty() => t,
            _ => {
                JWT_VALIDATIONS_TOTAL
                    .with_label_values(&[&ctx.channel_id, "missing"])
                    .inc();
                return send_error(session, 401, "MISSING_AUTH", "Authorization: Bearer <token> required").await;
            }
        };

        match validate_jwt(token, &cfg.global.jwt) {
            Ok(claims) => {
                JWT_VALIDATIONS_TOTAL
                    .with_label_values(&[&ctx.channel_id, "ok"])
                    .inc();
                ctx.tenant_id = claims.tid.clone();

                // Enforce that the JWT subject matches the requested channel.
                if claims.sub != ch_cfg.id {
                    JWT_VALIDATIONS_TOTAL
                        .with_label_values(&[&ctx.channel_id, "channel_mismatch"])
                        .inc();
                    return send_error(
                        session,
                        403,
                        "JWT_CHANNEL_MISMATCH",
                        "Token was not issued for this channel",
                    )
                    .await;
                }

                // Enforce that the JWT app_id matches the channel's allowed app_id.
                if claims.app != ch_cfg.app_id {
                    JWT_VALIDATIONS_TOTAL
                        .with_label_values(&[&ctx.channel_id, "app_mismatch"])
                        .inc();
                    return send_error(
                        session,
                        403,
                        "JWT_APP_MISMATCH",
                        "Token was not issued for this application",
                    )
                    .await;
                }

                ctx.app_id = Some(claims.app);
            }
            Err(e) => {
                let reason = if e.to_string().contains("expired") {
                    "expired"
                } else if e.to_string().contains("access") {
                    "wrong_type"
                } else {
                    "invalid"
                };
                JWT_VALIDATIONS_TOTAL
                    .with_label_values(&[&ctx.channel_id, reason])
                    .inc();
                return send_error(session, 401, "JWT_INVALID", &e.to_string()).await;
            }
        }

        // Pipeline continues → request_body_filter → upstream_peer
        Ok(false)
    }

    // ── Phase 2: upstream_peer ────────────────────────────────────────────────
    //
    // Select the backend HttpPeer from the shared load balancer.
    // The same worker pool handles both /auth/exchange and ingest routes.
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<HttpPeer>>
    where
        Self::CTX: Send + Sync,
    {
        let lb = self.discovery.load_balancer.load();

        // Round-robin selection. The key/hash parameters are ignored by RoundRobin.
        let backend = lb
            .select(b"", 256)
            .ok_or_else(|| pingora::Error::new_str("No available backends in load balancer"))?;

        // Internal workers are plain TCP without TLS/SNI.
        let peer = HttpPeer::new(backend.addr.clone(), false, String::new());

        tracing::debug!(
            channel = %ctx.channel_id,
            backend = %backend.addr,
            "Routing request to upstream"
        );

        Ok(Box::new(peer))
    }

    // ── Phase 3: upstream_request_filter ─────────────────────────────────────
    //
    // Modify the request headers sent upstream. Runs after the upstream
    // connection is established but before the body is forwarded.
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        // ── /auth/exchange path ───────────────────────────────────────────────
        // Forward the request completely unmodified so the worker receives the
        // original X-Channel-Id header and the full attestation body.
        if ctx.is_auth_exchange {
            return Ok(());
        }

        // ── Ingest path ───────────────────────────────────────────────────────
        
        // Security: strip the client-facing auth header — it MUST NOT leak
        // to the internal validation-worker service.
        upstream_request.remove_header("authorization");

        // Inject routing metadata using `&'static str` for the known static keys.
        upstream_request.insert_header("x-tenant-id", ctx.tenant_id.as_str())?;
        if let Some(app_id) = &ctx.app_id {
            upstream_request.insert_header("x-app-id", app_id.as_str())?;
        }

        Ok(())
    }

    // ── Phase 4: request_body_filter ─────────────────────────────────────────
    //
    // Called once per body chunk streaming from client → upstream.
    // Setting *body = None drops the chunk (not forwarded yet).
    // Setting *body = Some(bytes) forwards those bytes to upstream.
    //
    // Strategy: track running total of bytes to enforce `max_body_bytes` limit.
    // We treat the payload as an opaque blob and do NOT buffer or parse it here.
    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        // Already-rejected requests have no body to process.
        if ctx.reject.is_some() {
            *body = None;
            return Ok(());
        }

        // Track bytes to enforce max_body_bytes, but pass the chunk through unmodified.
        if let Some(chunk) = body {
            ctx.body_bytes_read += chunk.len();
            if ctx.body_bytes_read > ctx.max_body_bytes {
                let channel = ctx.channel_id.as_str();
                ENVELOPE_ERRORS_TOTAL
                    .with_label_values(&[channel, "too_large"])
                    .inc();
                ctx.reject = Some((413, "Payload exceeds size limit"));
                *body = None; // Drop this chunk
                return Ok(());
            }
        }

        if end_of_stream {
            let channel = ctx.channel_id.as_str();
            BODY_SIZE_BYTES
                .with_label_values(&[channel])
                .observe(ctx.body_bytes_read as f64);
        }

        Ok(())
    }

    // ── Phase 5: response_filter ──────────────────────────────────────────────
    //
    // Called when the upstream response headers arrive.
    // If body validation failed, override the upstream status with ours.
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if let Some((status, _message)) = ctx.reject {
            // The upstream saw an empty body and returned its own error.
            // We override that with our structured validation error.
            let code = StatusCode::from_u16(status)
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            upstream_response.set_status(code)?;
            upstream_response.remove_header("content-type");
            upstream_response.insert_header("content-type", "application/json")?;
            // content-length will be set by Pingora from response_body_filter output.
            upstream_response.remove_header("content-length");
        }

        upstream_response.insert_header("x-gateway", "logger-gateway/0.1")?;
        Ok(())
    }

    // ── Phase 6: response_body_filter ─────────────────────────────────────────
    //
    // Replace the upstream error body with our own JSON error message when
    // body validation failed.
    fn response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<Option<std::time::Duration>>
    where
        Self::CTX: Send + Sync,
    {
        if let Some((_, message)) = ctx.reject {
            // Discard all upstream body chunks.
            *body = if end_of_stream {
                // On the final chunk, emit our JSON error.
                Some(Bytes::from(format!(
                    r#"{{"error":"{}","code":"VALIDATION_ERROR"}}"#,
                    message
                )))
            } else {
                None
            };
        }
        // Return None → no artificial streaming delay.
        Ok(None)
    }

    // ── Phase 7: logging ──────────────────────────────────────────────────────
    //
    // Called after the response has been delivered (or on error). Emit metrics.
    async fn logging(
        &self,
        session: &mut Session,
        _error: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) where
        Self::CTX: Send + Sync,
    {
        let elapsed = ctx.start.elapsed().as_secs_f64();
        let channel = if ctx.channel_id.is_empty() {
            "unknown"
        } else {
            &ctx.channel_id
        };

        let status = if let Some((code, _)) = ctx.reject {
            code.to_string()
        } else {
            session
                .response_written()
                .map(|r| r.status.as_u16().to_string())
                .unwrap_or_else(|| "0".to_string())
        };

        REQUESTS_TOTAL
            .with_label_values(&[channel, &status])
            .inc();

        REQUEST_DURATION_SECONDS
            .with_label_values(&[channel])
            .observe(elapsed);

        tracing::info!(
            channel = %channel,
            status  = %status,
            elapsed_ms = %(elapsed * 1000.0) as u64,
            "Request completed"
        );
    }
}
