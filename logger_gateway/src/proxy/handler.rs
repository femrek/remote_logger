//! # Pingora ProxyHttp lifecycle implementation
//!
//! ## Request routing overview
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────┐
//! │  request_filter()                                                      │
//! │    ├─ path == "/auth/exchange"                                         │
//! │    │    ├─ Read X-Channel-Id header → find channel                     │
//! │    │    ├─ Read + buffer request body (≤ 2 MiB)                        │
//! │    │    ├─ Deserialise AttestationRequest                              │
//! │    │    ├─ [validate_attestation=true] verify Google/Apple token       │
//! │    │    ├─ Issue HS256 gateway JWT                                     │
//! │    │    └─ Write 200 JSON response → return Ok(true)  [PIPELINE STOP]  │
//! │    │                                                                   │
//! │    └─ path starts_with channel.path_prefix                             │
//! │         ├─ Validate Authorization: Bearer <jwt>                        │
//! │         ├─ Stash channel_id + tenant_id in ctx                         │
//! │         └─ return Ok(false)  [PIPELINE CONTINUES]                      │
//! │                                                                        │
//! │  upstream_peer()                                                       │
//! │    └─ Select HttpPeer from channel.backend config                      │
//! │                                                                        │
//! │  upstream_request_filter()                                             │
//! │    └─ Strip Authorization header, inject X-Tenant-Id + channel headers │
//! │                                                                        │
//! │  request_body_filter()  [called per chunk, streaming from client]      │
//! │    ├─ Accumulate chunks in ctx.body_buffer                             │
//! │    ├─ Size guard: reject if buffer > max_body_bytes                    │
//! │    └─ On end_of_stream:                                                │
//! │         ├─ parse_envelope() → shallow RawValue parse                   │
//! │         ├─ OK  → *body = Some(full_buffer) — forward to upstream       │
//! │         └─ Err → ctx.reject set, *body = None                          │
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
use bytes::{Bytes, BytesMut};
use pingora::http::{RequestHeader, ResponseHeader, StatusCode};
use pingora::proxy::{ProxyHttp, Session};
use pingora::upstreams::peer::HttpPeer;

use crate::auth::{
    AttestationRequest, AttestationVerifier, extract_bearer, issue_jwt, validate_jwt,
};
use crate::config::SharedConfig;
use crate::metrics::{
    ATTESTATION_TOTAL, BODY_SIZE_BYTES, ENVELOPE_ERRORS_TOTAL, JWT_VALIDATIONS_TOTAL,
    REQUEST_DURATION_SECONDS, REQUESTS_TOTAL,
};

// ─────────────────────────────────────────────────────────────────────────────
// Per-request context
// ─────────────────────────────────────────────────────────────────────────────

/// State that flows through the Pingora lifecycle for a single request.
pub struct RequestContext {
    /// Channel id resolved in `request_filter`. Empty string before resolution.
    pub channel_id: String,
    /// Tenant id from JWT claims (ingest path) or attestation request (exchange path).
    pub tenant_id: String,
    /// App id from the parsed envelope (available after `request_body_filter` on EOS).
    pub app_id: Option<String>,

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

    // ── Short-circuit flag ───────────────────────────────────────────────────
    /// `true` for `/auth/exchange` requests — `request_filter` handled them
    /// completely and returned `Ok(true)`, so no upstream is contacted.
    pub is_short_circuit: bool,

    // ── Observability ────────────────────────────────────────────────────────
    pub start: Instant,
}

impl Default for RequestContext {
    fn default() -> Self {
        Self {
            channel_id: String::new(),
            tenant_id: String::new(),
            app_id: None,
            body_bytes_read: 0,
            max_body_bytes: 2 * 1024 * 1024,
            reject: None,
            is_short_circuit: false,
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
    /// Platform-dispatching attestation verifier (shared across all requests).
    pub verifier: Arc<dyn AttestationVerifier>,
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

/// Read the full request body up to `max_bytes` from `session`.
///
/// ## Body lifecycle note
/// Calling this in `request_filter` consumes the bytes from the underlying
/// TCP/TLS stream. This is intentional for `/auth/exchange` requests because
/// they are short-circuited — Pingora never attempts to proxy them upstream.
///
/// For ingest requests that *do* forward to upstream, body consumption happens
/// in `request_body_filter` so Pingora can stream the validated bytes.
async fn read_body_capped(session: &mut Session, max_bytes: usize) -> pingora::Result<Bytes> {
    let mut buf = BytesMut::new();
    loop {
        match session.read_request_body().await? {
            Some(chunk) => {
                if buf.len() + chunk.len() > max_bytes {
                    return Err(pingora::Error::new_str("request body too large"));
                }
                buf.extend_from_slice(&chunk);
            }
            None => return Ok(buf.freeze()),
        }
    }
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
        ctx.max_body_bytes = cfg.gateway.max_body_bytes;

        let path = session.req_header().uri.path().to_owned();

        // ── /auth/exchange ────────────────────────────────────────────────────
        if path == "/auth/exchange" {
            return self.handle_exchange(session, ctx, &cfg).await;
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

        match validate_jwt(token, &cfg.gateway.jwt) {
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
    // Select the backend HttpPeer for this channel.
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<HttpPeer>>
    where
        Self::CTX: Send + Sync,
    {
        let cfg = self.config.load();
        let ch_cfg = cfg
            .find_channel_by_id(&ctx.channel_id)
            .expect("channel_id set in request_filter; channel lookup must succeed");

        // Parse "host:port" → SocketAddr.
        //
        // Production note: for hostname-based backends (not IP), use
        // `tokio::net::lookup_host` to resolve and round-robin across the
        // returned addresses, or integrate Pingora's built-in LoadBalancer.
        let addr: std::net::SocketAddr = ch_cfg
            .backend
            .addr
            .parse()
            .map_err(|e| pingora::Error::new_str(Box::leak(
                format!("backend addr parse failed '{}': {e}", ch_cfg.backend.addr).into_boxed_str()
            )))?;

        let peer = HttpPeer::new(addr, ch_cfg.backend.tls, ch_cfg.backend.sni.clone());

        tracing::debug!(
            channel = %ctx.channel_id,
            backend = %ch_cfg.backend.addr,
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
        // Collect all config-sourced header pairs as owned Strings while holding
        // the ArcSwap guard, then drop the guard before any insert_header calls.
        //
        // Why: `IntoCaseHeaderName` in pingora-http 0.8 is only implemented for
        // `String` and `&'static str` — not for arbitrary `&str`. Collecting
        // owned Strings lets us avoid the `'static` bound on dynamic header names.
        let channel_headers: Vec<(String, String)> = {
            let cfg = self.config.load();
            let ch_cfg = cfg
                .find_channel_by_id(&ctx.channel_id)
                .expect("channel exists; was validated in request_filter");
            ch_cfg
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
            // cfg guard dropped here
        };

        // Security: strip the client-facing auth header — it MUST NOT leak
        // to the internal Kafka-writer service.
        upstream_request.remove_header("authorization");

        // Inject routing metadata using `&'static str` for the known static keys.
        upstream_request.insert_header("x-tenant-id", ctx.tenant_id.as_str())?;
        if let Some(app_id) = &ctx.app_id {
            upstream_request.insert_header("x-app-id", app_id.as_str())?;
        }

        // Inject channel-specific headers from config (topic, environment, …).
        // `key` is an owned `String` which satisfies `IntoCaseHeaderName`.
        for (key, value) in channel_headers {
            upstream_request.insert_header(key, value.as_str())?;
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
    // We treat the payload as an opaque blob and do NOT buffer or parse it here;
    // validation (such as app_id) is handled via JWT claims during `request_filter`.
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
        // Short-circuit or already-rejected requests have no body to process.
        if ctx.is_short_circuit || ctx.reject.is_some() {
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

// ─────────────────────────────────────────────────────────────────────────────
// /auth/exchange handler (called from request_filter, short-circuits)
// ─────────────────────────────────────────────────────────────────────────────

impl LogGateway {
    /// Handle a session-exchange request:
    /// 1. Identify the channel via `X-Channel-Id` header.
    /// 2. Optionally verify the hardware attestation token.
    /// 3. Issue a short-lived gateway JWT.
    /// 4. Write `{"token":"<jwt>"}` and return `Ok(true)` to stop the pipeline.
    async fn handle_exchange(
        &self,
        session: &mut Session,
        ctx: &mut RequestContext,
        cfg: &crate::config::GatewayConfig,
    ) -> pingora::Result<bool> {
        ctx.is_short_circuit = true;

        // Resolve channel from X-Channel-Id header.
        let channel_id = match session
            .req_header()
            .headers
            .get("x-channel-id")
            .and_then(|v| v.to_str().ok())
        {
            Some(id) if !id.is_empty() => id.to_owned(),
            _ => {
                return send_error(
                    session,
                    400,
                    "MISSING_CHANNEL_HEADER",
                    "X-Channel-Id header is required for /auth/exchange",
                )
                .await;
            }
        };

        let ch_cfg = match cfg.find_channel_by_id(&channel_id) {
            Some(ch) => ch.clone(),
            None => {
                return send_error(
                    session,
                    404,
                    "CHANNEL_NOT_FOUND",
                    &format!("Unknown channel: {channel_id}"),
                )
                .await;
            }
        };

        ctx.channel_id = ch_cfg.id.clone();

        // Read and parse the attestation request body.
        let body = match read_body_capped(session, cfg.gateway.max_body_bytes).await {
            Ok(b) => b,
            Err(_) => {
                return send_error(session, 413, "PAYLOAD_TOO_LARGE", "Request body too large")
                    .await;
            }
        };

        let attest_req: AttestationRequest = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                return send_error(
                    session,
                    400,
                    "INVALID_REQUEST",
                    &format!("Failed to parse attestation request: {e}"),
                )
                .await;
            }
        };

        ctx.tenant_id = attest_req.tenant_id.clone();

        // Verify attestation token (or skip if disabled for this channel).
        if ch_cfg.validate_attestation {
            if let Err(e) = self
                .verifier
                .verify(
                    &attest_req.attestation_token,
                    attest_req.platform,
                    &attest_req.tenant_id,
                    &ch_cfg.app_id,
                    ch_cfg.google_project_number.as_deref(),
                    ch_cfg.apple_team_id.as_deref(),
                )
                .await
            {
                ATTESTATION_TOTAL
                    .with_label_values(&[&ch_cfg.id, "failed"])
                    .inc();
                return send_error(session, 403, "ATTESTATION_FAILED", &e.to_string()).await;
            }
            ATTESTATION_TOTAL
                .with_label_values(&[&ch_cfg.id, "ok"])
                .inc();
        } else {
            ATTESTATION_TOTAL
                .with_label_values(&[&ch_cfg.id, "skipped"])
                .inc();
        }

        // Issue the gateway JWT.
        let jwt = match issue_jwt(
            &ch_cfg.id,
            &attest_req.tenant_id,
            &attest_req.platform.to_string(),
            &ch_cfg.app_id,
            &cfg.gateway.jwt,
        ) {
            Ok(t) => t,
            Err(e) => {
                return send_error(session, 500, "INTERNAL_ERROR", &e.to_string()).await;
            }
        };

        // Write 200 response with the JWT.
        let response_body = Bytes::from(format!(
            r#"{{"token":"{jwt}","expires_in":{ttl}}}"#,
            ttl = cfg.gateway.jwt.ttl_seconds,
        ));

        let mut resp = ResponseHeader::build(StatusCode::OK, None)?;
        resp.insert_header("content-type", "application/json")?;
        resp.insert_header("content-length", response_body.len().to_string())?;
        resp.insert_header("x-gateway", "logger-gateway/0.1")?;

        session.write_response_header(Box::new(resp), false).await?;
        session
            .write_response_body(Some(response_body), true)
            .await?;

        tracing::info!(
            channel = %ch_cfg.id,
            tenant  = %attest_req.tenant_id,
            platform = %attest_req.platform,
            ttl_s   = cfg.gateway.jwt.ttl_seconds,
            "JWT issued"
        );

        // Return true → Pingora stops the pipeline; no upstream is contacted.
        Ok(true)
    }
}
