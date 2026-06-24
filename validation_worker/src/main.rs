//! Binary entry-point for the Stateless Validation Worker.
//!
//! All business logic lives in `lib.rs`. This file is intentionally minimal:
//! it reads environment variables, wires up the real [`RdkafkaSink`], and
//! hands control to the shared [`build_app`] function.

use std::sync::Arc;
use validation_worker::{build_app, AppState, RdkafkaSink};

#[tokio::main]
async fn main() {
    // Initialise tracing; respects RUST_LOG env var.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let kafka_brokers =
        std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

    let sink = RdkafkaSink::new(&kafka_brokers).expect("Failed to create Kafka producer");

    let state = Arc::new(AppState {
        sink: Arc::new(sink),
        kafka_topic: "mobile-logs".to_string(),
    });

    let app = build_app(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Stateless Validation Worker listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
