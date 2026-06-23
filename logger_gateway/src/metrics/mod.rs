use lazy_static::lazy_static;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

// ─────────────────────────────────────────────────────────────────────────────
// Registry & metric definitions
// ─────────────────────────────────────────────────────────────────────────────

lazy_static! {
    /// Dedicated Prometheus registry — keeps our metrics separate from any
    /// third-party libraries that register to the default global registry.
    pub static ref REGISTRY: Registry = Registry::new();

    // ── Request throughput ────────────────────────────────────────────────────

    /// Total requests processed, labelled by channel and HTTP status code.
    ///
    /// Example query: `rate(gateway_requests_total[5m])`
    pub static ref REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "gateway_requests_total",
            "Total HTTP requests processed by the gateway",
        ),
        &["channel", "status_code"],
    )
    .expect("Failed to create REQUESTS_TOTAL metric");

    // ── Latency ───────────────────────────────────────────────────────────────

    /// End-to-end request latency histogram (seconds), by channel.
    ///
    /// Covers the full ProxyHttp lifecycle: `request_filter` → upstream round-trip
    /// → response delivered to client.
    pub static ref REQUEST_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "gateway_request_duration_seconds",
            "End-to-end request processing duration in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]),
        &["channel"],
    )
    .expect("Failed to create REQUEST_DURATION_SECONDS metric");

    // ── Auth / JWT ────────────────────────────────────────────────────────────

    /// JWT validation outcomes for ingest requests.
    ///
    /// `result` label values:  `ok` | `expired` | `invalid` | `missing`
    pub static ref JWT_VALIDATIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "gateway_jwt_validations_total",
            "JWT validation results for ingest endpoint requests",
        ),
        &["channel", "result"],
    )
    .expect("Failed to create JWT_VALIDATIONS_TOTAL metric");

    /// Hardware attestation verification outcomes for `/auth/exchange`.
    ///
    /// `result` label values:  `ok` | `failed` | `skipped`
    pub static ref ATTESTATION_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "gateway_attestation_total",
            "Hardware attestation verification outcomes at /auth/exchange",
        ),
        &["channel", "result"],
    )
    .expect("Failed to create ATTESTATION_TOTAL metric");

    // ── Envelope ──────────────────────────────────────────────────────────────

    /// Ingest request body size distribution (bytes), by channel.
    pub static ref BODY_SIZE_BYTES: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "gateway_body_size_bytes",
            "Ingest request body size distribution in bytes",
        )
        .buckets(vec![
            1_024.0,      //   1 KiB
            10_240.0,     //  10 KiB
            51_200.0,     //  50 KiB
            102_400.0,    // 100 KiB
            524_288.0,    // 512 KiB
            1_048_576.0,  //   1 MiB
            2_097_152.0,  //   2 MiB (hard limit)
        ]),
        &["channel"],
    )
    .expect("Failed to create BODY_SIZE_BYTES metric");

    /// Envelope validation failures, labelled by channel and failure reason.
    ///
    /// `reason` label values:  `too_large` | `parse_error` | `missing_field`
    pub static ref ENVELOPE_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "gateway_envelope_errors_total",
            "Log envelope validation failures before upstream forwarding",
        ),
        &["channel", "reason"],
    )
    .expect("Failed to create ENVELOPE_ERRORS_TOTAL metric");
}

// ─────────────────────────────────────────────────────────────────────────────
// Registration
// ─────────────────────────────────────────────────────────────────────────────

/// Register all gateway metrics into `REGISTRY`.
///
/// Must be called once at startup before any metrics are recorded.
/// Repeated calls return an error (safe to ignore in tests via `let _ =`).
pub fn register_all() -> anyhow::Result<()> {
    REGISTRY.register(Box::new(REQUESTS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(REQUEST_DURATION_SECONDS.clone()))?;
    REGISTRY.register(Box::new(JWT_VALIDATIONS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(ATTESTATION_TOTAL.clone()))?;
    REGISTRY.register(Box::new(BODY_SIZE_BYTES.clone()))?;
    REGISTRY.register(Box::new(ENVELOPE_ERRORS_TOTAL.clone()))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Gather
// ─────────────────────────────────────────────────────────────────────────────

/// Encode all registered metrics as the Prometheus text exposition format (0.0.4).
pub fn gather_text() -> anyhow::Result<String> {
    let encoder = TextEncoder::new();
    let mut buf = Vec::new();

    // Custom gateway metrics
    encoder.encode(&REGISTRY.gather(), &mut buf)?;

    // Default process metrics (memory, CPU, open FDs) from the global registry
    encoder.encode(&prometheus::gather(), &mut buf)?;

    Ok(String::from_utf8(buf)?)
}

// ─────────────────────────────────────────────────────────────────────────────
// Background scrape server
// ─────────────────────────────────────────────────────────────────────────────

/// Run a minimal raw-TCP HTTP/1.1 server that serves `GET /metrics`.
///
/// This intentionally has no external HTTP framework dependency — it reuses
/// Tokio's TCP primitives to keep the binary lean. The endpoint should be
/// on an **internal** port, never exposed to the public internet.
pub async fn serve_metrics(addr: &str) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "Prometheus /metrics endpoint listening");

    loop {
        let (mut stream, peer) = listener.accept().await?;

        tokio::spawn(async move {
            // Read the HTTP request (we only need to know the client sent *something*).
            let mut req_buf = [0u8; 512];
            let _ = stream.read(&mut req_buf).await;

            let body = match gather_text() {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to gather metrics");
                    format!("# error gathering metrics: {e}\n")
                }
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {}",
                body.len(),
                body
            );

            if let Err(e) = stream.write_all(response.as_bytes()).await {
                tracing::debug!(peer = %peer, error = %e, "Metrics write error");
            }
        });
    }
}
