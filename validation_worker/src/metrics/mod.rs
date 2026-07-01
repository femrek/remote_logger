//! Prometheus metrics for the validation worker.
//!
//! Exposes a dedicated private [`Registry`] (separate from any third-party
//! crates that register to the global registry) and a minimal raw-TCP HTTP/1.1
//! server that serves `GET /metrics` — mirroring the pattern used by the
//! `logger_gateway` metrics module.

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

    /// Total requests processed by the worker, labelled by channel and HTTP status.
    ///
    /// Example query: `rate(worker_requests_total[5m])`
    pub static ref REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "worker_requests_total",
            "Total HTTP requests processed by the validation worker",
        ),
        &["channel", "status_code"],
    )
    .expect("Failed to create REQUESTS_TOTAL metric");

    // ── Latency ───────────────────────────────────────────────────────────────

    /// End-to-end request latency histogram (seconds), by channel.
    pub static ref REQUEST_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "worker_request_duration_seconds",
            "End-to-end request processing duration in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]),
        &["channel"],
    )
    .expect("Failed to create REQUEST_DURATION_SECONDS metric");

    // ── Kafka produce ─────────────────────────────────────────────────────────

    /// Total Kafka produce attempts, labelled by topic and result.
    ///
    /// `result` label values: `ok` | `err`
    pub static ref KAFKA_PRODUCE_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "worker_kafka_produce_total",
            "Total Kafka produce attempts by the validation worker",
        ),
        &["topic", "result"],
    )
    .expect("Failed to create KAFKA_PRODUCE_TOTAL metric");

    /// Time from produce call to broker ACK (seconds), by topic.
    pub static ref KAFKA_PRODUCE_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "worker_kafka_produce_duration_seconds",
            "Time from Kafka produce call to broker ACK in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]),
        &["topic"],
    )
    .expect("Failed to create KAFKA_PRODUCE_DURATION_SECONDS metric");

    // ── JWT operations ────────────────────────────────────────────────────────

    /// JWT minting outcomes, labelled by operation and result.
    ///
    /// `operation` values: `mint_access` | `mint_refresh`
    /// `result`    values: `ok` | `err`
    pub static ref JWT_OPERATIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "worker_jwt_operations_total",
            "JWT minting operations performed by the validation worker",
        ),
        &["operation", "result"],
    )
    .expect("Failed to create JWT_OPERATIONS_TOTAL metric");
}

// ─────────────────────────────────────────────────────────────────────────────
// Registration
// ─────────────────────────────────────────────────────────────────────────────

/// Register all worker metrics into [`REGISTRY`].
///
/// Must be called once at startup before any metrics are recorded.
/// Repeated calls return an error (safe to ignore in tests via `let _ =`).
pub fn register_all() -> anyhow::Result<()> {
    REGISTRY.register(Box::new(REQUESTS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(REQUEST_DURATION_SECONDS.clone()))?;
    REGISTRY.register(Box::new(KAFKA_PRODUCE_TOTAL.clone()))?;
    REGISTRY.register(Box::new(KAFKA_PRODUCE_DURATION_SECONDS.clone()))?;
    REGISTRY.register(Box::new(JWT_OPERATIONS_TOTAL.clone()))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Gather
// ─────────────────────────────────────────────────────────────────────────────

/// Encode all registered metrics as Prometheus text exposition format (0.0.4).
pub fn gather_text() -> anyhow::Result<String> {
    let encoder = TextEncoder::new();
    let mut buf = Vec::new();

    // Custom worker metrics
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
            // Read the HTTP request (we only need to know the client sent something).
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
