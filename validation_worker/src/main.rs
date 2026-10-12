//! Binary entry-point for the Stateless Validation Worker.

use std::sync::Arc;

use validation_worker::{build_app, AppState, RdkafkaSink, metrics};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    // ── Tracing ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── Prometheus metrics registration ───────────────────────────────────────
    if let Err(e) = metrics::register_all() {
        tracing::error!(error = %e, "Failed to register Prometheus metrics");
    }

    // ── Environment variables ─────────────────────────────────────────────────
    let kafka_brokers = env_or("KAFKA_BROKERS", "localhost:9092");
    let kafka_topic   = env_or("KAFKA_TOPIC", "mobile-logs");
    let port          = env_or("PORT", "8000");
    let metrics_port  = env_or("METRICS_PORT", "9091");
    let consul_addr   = env_or("CONSUL_HTTP_ADDR", "http://localhost:8500");

    let mut advertised_ip = env_or("ADVERTISED_IP", "auto");
    if advertised_ip == "auto" {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    advertised_ip = addr.ip().to_string();
                }
            }
        }
    }

    let mut node_id = env_or("NODE_ID", "auto");
    if node_id == "auto" {
        node_id = std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| format!("worker-{}", std::process::id()))
            .trim()
            .to_string();
    }

    // ── Kafka producer ────────────────────────────────────────────────────────
    let sink = RdkafkaSink::new(&kafka_brokers).expect("Failed to create Kafka producer");

    // ── Application state ─────────────────────────────────────────────────────
    let state = Arc::new(AppState {
        sink: Arc::new(sink),
        kafka_topic,
    });

    let app = build_app(state);

    // ── Spawn Prometheus metrics server ───────────────────────────────────────
    let metrics_addr = format!("0.0.0.0:{metrics_port}");
    tokio::spawn(async move {
        if let Err(e) = metrics::serve_metrics(&metrics_addr).await {
            tracing::error!(error = %e, "Metrics server error");
        }
    });
    tracing::info!(port = %metrics_port, "Prometheus /metrics available on port {}", metrics_port);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    // ── Consul Service Registration ───────────────────────────────────────────
    let parsed_port = port.parse::<u16>().unwrap_or(8000);
    tokio::spawn({
        let consul_addr = consul_addr.clone();
        let node_id = node_id.clone();
        let advertised_ip = advertised_ip.clone();
        async move {
            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "ID": node_id,
                "Name": "validation_worker",
                "Address": advertised_ip,
                "Port": parsed_port,
                "Check": {
                    "HTTP": format!("http://{}:{}/health", advertised_ip, parsed_port),
                    "Interval": "10s",
                    "DeregisterCriticalServiceAfter": "1m",
                    "Timeout": "5s"
                }
            });
            
            let mut first_success = true;
            loop {
                match client.put(&format!("{}/v1/agent/service/register", consul_addr))
                    .json(&payload)
                    .send().await {
                    Ok(res) if res.status().is_success() => {
                        if first_success {
                            tracing::info!("Successfully registered validation_worker with Consul at {}", consul_addr);
                            first_success = false;
                        } else {
                            tracing::trace!("Refreshed validation_worker Consul registration at {}", consul_addr);
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    }
                    Ok(res) => {
                        tracing::warn!("Failed to register with Consul (status: {}), retrying in 3s...", res.status());
                        first_success = true;
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                    Err(err) => {
                        tracing::warn!("Reqwest error registering with Consul: {}, retrying in 3s...", err);
                        first_success = true;
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }
            }
        }
    });

    tracing::info!("Stateless Validation Worker listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
