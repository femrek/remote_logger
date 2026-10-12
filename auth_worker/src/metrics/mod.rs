//! Prometheus metrics for the auth worker.

use lazy_static::lazy_static;
use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    pub static ref REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "auth_worker_requests_total",
            "Total HTTP requests processed by the auth worker",
        ),
        &["channel", "status_code"],
    )
    .expect("Failed to create REQUESTS_TOTAL metric");

    pub static ref REQUEST_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "auth_worker_request_duration_seconds",
            "End-to-end request processing duration in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]),
        &["channel"],
    )
    .expect("Failed to create REQUEST_DURATION_SECONDS metric");

    pub static ref KAFKA_PRODUCE_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "auth_worker_kafka_produce_total",
            "Total Kafka produce attempts by the auth worker",
        ),
        &["topic", "result"],
    )
    .expect("Failed to create KAFKA_PRODUCE_TOTAL metric");

    pub static ref KAFKA_PRODUCE_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "auth_worker_kafka_produce_duration_seconds",
            "Time from Kafka produce call to broker ACK in seconds",
        )
        .buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
        ]),
        &["topic"],
    )
    .expect("Failed to create KAFKA_PRODUCE_DURATION_SECONDS metric");

    pub static ref JWT_OPERATIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "auth_worker_jwt_operations_total",
            "JWT minting operations performed by the auth worker",
        ),
        &["operation", "result"],
    )
    .expect("Failed to create JWT_OPERATIONS_TOTAL metric");
}

pub fn register_all() -> anyhow::Result<()> {
    REGISTRY.register(Box::new(REQUESTS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(REQUEST_DURATION_SECONDS.clone()))?;
    REGISTRY.register(Box::new(KAFKA_PRODUCE_TOTAL.clone()))?;
    REGISTRY.register(Box::new(KAFKA_PRODUCE_DURATION_SECONDS.clone()))?;
    REGISTRY.register(Box::new(JWT_OPERATIONS_TOTAL.clone()))?;
    Ok(())
}

pub fn gather_text() -> anyhow::Result<String> {
    let encoder = TextEncoder::new();
    let mut buf = Vec::new();
    encoder.encode(&REGISTRY.gather(), &mut buf)?;
    encoder.encode(&prometheus::gather(), &mut buf)?;
    Ok(String::from_utf8(buf)?)
}

pub async fn serve_metrics(addr: &str) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "Prometheus /metrics endpoint listening");

    loop {
        let (mut stream, peer) = listener.accept().await?;

        tokio::spawn(async move {
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
