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
// Strict Rust struct representing a ServerLog
#[derive(Debug, Deserialize, Serialize)]
struct DeviceInfo {
    os: String,
    os_version: String,
    device_model: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerLog {
    timestamp: String,
    level: String,
    message: String,
    device_info: DeviceInfo,
}

// Struct to represent the final payload sent to Kafka
#[derive(Debug, Serialize)]
struct KafkaPayload {
    tenant_id: String,
    app_id: String,
    #[serde(flatten)]
    log: ServerLog,
}

#[tokio::main]
async fn main() {
    // Initialize Kafka producer
    // You can adjust these settings based on your cluster configuration.
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("message.timeout.ms", "5000")
        .create()
        .expect("Producer creation error");

    let state = AppState {
        kafka_producer: producer,
        kafka_topic: "mobile-logs".to_string(),
    };

    // Build the Axum application
    let app = Router::new()
        .route("/ingest", post(ingest_handler))
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Stateless Validation Worker listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

// HTTP Endpoint: POST /ingest
async fn ingest_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    // Using axum::Json for automatic JSON parsing & validation.
    // If JSON is malformed or missing fields, Axum automatically returns a 400 Bad Request.
    // We wrap it in a custom error to ensure a clean 400 response if desired,
    // but standard `Json<ServerLog>` extractor does exactly what is requested.
    payload: Result<Json<ServerLog>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    // 3. Deep JSON Validation
    // If the JSON is malformed or missing required fields, immediately return a 400 Bad Request.
    let Json(log_payload) = match payload {
        Ok(parsed) => parsed,
        Err(_) => return Err(AppError::BadRequest),
    };

    // 2. Header Extraction
    // Read x-tenant-id and x-app-id from the request headers.
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    let app_id = headers
        .get("x-app-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if tenant_id.is_empty() || app_id.is_empty() {
        // You might want to return 400 if headers are strictly required.
        return Err(AppError::BadRequest);
    }

    // 4. Kafka Production
    let kafka_payload = KafkaPayload {
        tenant_id: tenant_id.clone(),
        app_id,
        log: log_payload,
    };

    let serialized_payload = match serde_json::to_string(&kafka_payload) {
        Ok(s) => s,
        Err(_) => return Err(AppError::InternalError),
    };

    // 5. Zero Data Loss Guarantee
    // Await the delivery acknowledgment (ACK) from the rdkafka FutureProducer.
    let record = FutureRecord::to(&state.kafka_topic)
        .key(&tenant_id) // Optional: use tenant_id as routing key
        .payload(&serialized_payload);

    match state.kafka_producer.send(record, Duration::from_secs(5)).await {
        Ok((partition, offset)) => {
            println!("Delivered message to partition {} at offset {}", partition, offset);
            Ok(StatusCode::OK)
        }
        Err((e, _)) => {
            eprintln!("Failed to deliver message to Kafka: {:?}", e);
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
