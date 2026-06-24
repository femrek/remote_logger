use axum::{
    extract::{State, Json},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    kafka_producer: FutureProducer,
    kafka_topic: String,
}

// 3. Deep JSON Validation
// Strict Rust struct representing a ServerLog (production schema matching openapi.json & mobile client)
#[derive(Debug, Deserialize, Serialize)]
struct ServerLog {
    timestamp: String,
    #[serde(rename = "loggerName")]
    logger_name: String,
    message: String,
    level: String,
    zone: String,
    #[serde(rename = "deviceUUID")]
    device_uuid: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    error: Option<String>,
    stacktrace: Option<String>,
}

// Shallow envelope struct corresponding to LogEnvelope in gateway configuration
#[derive(Debug, Deserialize)]
struct IngestEnvelope {
    app_id: String,
    tenant_id: String,
    payload: serde_json::Value,
}

// Struct to represent the final payload sent to Kafka
#[derive(Debug, Serialize)]
struct KafkaPayload {
    tenant_id: String,
    app_id: String,
    log_index_prefix: String,
    #[serde(flatten)]
    log: ServerLog,
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Read Kafka brokers from environment
    let kafka_brokers = std::env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

    // Initialize Kafka producer
    // You can adjust these settings based on your cluster configuration.
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &kafka_brokers)
        .set("message.timeout.ms", "5000")
        .set("reconnect.backoff.ms", "10000")
        .set("reconnect.backoff.max.ms", "10000")
        .create()
        .expect("Producer creation error");

    let state = AppState {
        kafka_producer: producer,
        kafka_topic: "mobile-logs".to_string(),
    };

    // Build the Axum application
    let app = Router::new()
        .route("/ingest", post(ingest_handler))
        .route("/v1/logs/:channel", post(ingest_handler))
        .with_state(Arc::new(state));

    // Read port from environment
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Stateless Validation Worker listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

// HTTP Endpoint: POST /ingest
async fn ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    // Using axum::Json for automatic JSON parsing & validation.
    // We wrap it in a custom error to log precisely why it failed.
    payload: Result<Json<IngestEnvelope>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    // 2. Header Extraction
    // Read x-tenant-id and x-app-id from the request headers.
    let mut tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let mut app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    // Log the incoming request details (headers only at this stage)
    tracing::info!(
        tenant_id = %tenant_id,
        app_id = %app_id,
        "Received ingest request"
    );

    // 3. Deep JSON Validation
    // Parse the envelope body
    let Json(envelope) = match payload {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                app_id = %app_id,
                error = %err,
                "Failed to parse IngestEnvelope JSON body"
            );
            return Err(AppError::BadRequest);
        }
    };

    // If headers are missing, fall back to envelope fields
    if tenant_id.is_empty() {
        tenant_id = envelope.tenant_id.clone();
    }
    if app_id.is_empty() {
        app_id = envelope.app_id.clone();
    }

    if tenant_id.is_empty() || app_id.is_empty() {
        tracing::warn!(
            tenant_id = %tenant_id,
            app_id = %app_id,
            "Rejected request: missing x-tenant-id or x-app-id identifiers"
        );
        return Err(AppError::BadRequest);
    }

    // Perform deep validation of the inner payload against production ServerLog schema
    let log_payload: ServerLog = match serde_json::from_value(envelope.payload) {
        Ok(parsed) => parsed,
        Err(err) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                app_id = %app_id,
                error = %err,
                "Failed to validate inner log payload against strict ServerLog schema"
            );
            return Err(AppError::BadRequest);
        }
    };

    // 4. Kafka Production
    let kafka_payload = KafkaPayload {
        tenant_id: tenant_id.clone(),
        app_id: app_id.clone(),
        log_index_prefix: format!("mobile-logs-{}", app_id),
        log: log_payload,
    };

    let serialized_payload = match serde_json::to_string(&kafka_payload) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(
                tenant_id = %tenant_id,
                error = %err,
                "Failed to serialize payload to JSON"
            );
            return Err(AppError::InternalError);
        }
    };

    // 5. Zero Data Loss Guarantee
    // Await the delivery acknowledgment (ACK) from the rdkafka FutureProducer.
    let record = FutureRecord::to(&state.kafka_topic)
        .key(&tenant_id) // Optional: use tenant_id as routing key
        .payload(&serialized_payload);

    match state.kafka_producer.send(record, Duration::from_secs(5)).await {
        Ok((partition, offset)) => {
            tracing::info!(
                tenant_id = %tenant_id,
                partition = partition,
                offset = offset,
                "Delivered message to Kafka successfully"
            );
            Ok(StatusCode::OK)
        }
        Err((e, _)) => {
            tracing::error!(
                tenant_id = %tenant_id,
                error = ?e,
                "Failed to deliver message to Kafka"
            );
            // If the Kafka broker fails or times out, return a 503 Service Unavailable
            Err(AppError::ServiceUnavailable)
        }
    }
}

// Custom error type to handle HTTP status codes elegantly
enum AppError {
    BadRequest,
    ServiceUnavailable,
    InternalError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::BadRequest => (StatusCode::BAD_REQUEST, "Bad Request"),
            AppError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable"),
            AppError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };

        (status, error_message).into_response()
    }
}
