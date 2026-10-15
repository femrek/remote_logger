use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Extension, Path, State},
    http::{Response, StatusCode, header},
    middleware,
    response::{IntoResponse, Json},
    routing::{delete, get},
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

use crate::auth::{AdminToken, bearer_auth_middleware};
use crate::consul::ConsulClient;

// ─────────────────────────────────────────────────────────────────────────────
// Static asset embedding
// ─────────────────────────────────────────────────────────────────────────────

#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAssets;

// ─────────────────────────────────────────────────────────────────────────────
// Shared app state
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub consul: Arc<ConsulClient>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Config types (mirrored from logger_gateway schema)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    #[serde(default = "default_issuer")]
    pub issuer: String,
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: u64,
}

fn default_issuer() -> String { "logger-gateway".to_string() }
fn default_ttl_seconds() -> u64 { 900 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub jwt: JwtConfig,
    #[serde(default = "default_max_body")]
    pub max_body_bytes: usize,
}

fn default_max_body() -> usize { 2 * 1024 * 1024 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub id: String,
    pub app_id: String,
    /// Android attestation mode: off | monitor | enforce.
    #[serde(default)]
    pub android_attestation_mode: Option<String>,
    /// iOS attestation mode: off | monitor | enforce.
    #[serde(default)]
    pub ios_attestation_mode: Option<String>,
    /// Legacy boolean flag for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_attestation: Option<bool>,
    /// Google Cloud Project Number for Android Play Integrity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_project_number: Option<String>,
    /// Apple Developer Team ID for App Attest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_team_id: Option<String>,
    /// Android release/play signing certificate SHA-256 digests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub android_signing_cert_sha256: Vec<String>,
    /// Apple App Attest environment ("development" | "production").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_app_attest_env: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// API request / response types
// ─────────────────────────────────────────────────────────────────────────────

/// Response for GET /api/config — includes the config values plus
/// ModifyIndexes needed by the client for CAS writes.
#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub global: GlobalConfig,
    pub global_modify_index: u64,
    pub channels: Vec<ChannelWithIndex>,
}

#[derive(Debug, Serialize)]
pub struct ChannelWithIndex {
    #[serde(flatten)]
    pub channel: ChannelConfig,
    pub modify_index: u64,
}

/// Request body for POST /api/config.
#[derive(Debug, Deserialize)]
pub struct SaveConfigRequest {
    pub global: GlobalConfig,
    /// ModifyIndex for `gateway/config/global` as returned by GET.
    pub global_modify_index: u64,
    /// Unified channel list — includes both routing (id, app_id) and
    /// attestation fields. Written to `gateway/config/channels/<id>` in Consul.
    pub channels: Vec<ChannelConfig>,
    /// ModifyIndexes per channel id. Missing entries default to 0 (create-new).
    #[serde(default)]
    pub channel_modify_indexes: std::collections::HashMap<String, u64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_router(consul: Arc<ConsulClient>, token: Arc<AdminToken>) -> Router {
    let state = AppState { consul };

    // Protected API routes — require Bearer token.
    let api = Router::new()
        .route("/config", get(get_config).post(save_config))
        .route("/config/channels/{id}", delete(delete_channel))
        .layer(middleware::from_fn(bearer_auth_middleware))
        .layer(Extension(token));

    Router::new()
        // Public routes
        .route("/health", get(health))
        .route("/", get(serve_index))
        .route("/static/{*file}", get(serve_static))
        // Protected API
        .nest("/api", api)
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers — public
// ─────────────────────────────────────────────────────────────────────────────

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn serve_index() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

async fn serve_static(Path(file): Path<String>) -> impl IntoResponse {
    serve_embedded_file(&file)
}

fn serve_embedded_file(path: &str) -> Response<Body> {
    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers — protected API
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/config
///
/// Returns the current gateway config assembled from Consul KV,
/// including ModifyIndexes for subsequent CAS writes.
async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<ConfigResponse>, (StatusCode, String)> {
    let entries = state
        .consul
        .get_prefix("gateway/config/")
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let mut global: Option<GlobalConfig> = None;
    let mut global_modify_index: u64 = 0;
    let mut channels: Vec<ChannelWithIndex> = Vec::new();

    for entry in &entries {
        let bytes = entry
            .decoded_value()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if entry.key == "gateway/config/global" {
            let g: GlobalConfig = serde_json::from_slice(&bytes)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            global = Some(g);
            global_modify_index = entry.modify_index;
        } else if entry.key.starts_with("gateway/config/channels/") {
            let ch: ChannelConfig = serde_json::from_slice(&bytes)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            channels.push(ChannelWithIndex {
                channel: ch,
                modify_index: entry.modify_index,
            });
        }
    }

    let global = global.unwrap_or_else(|| GlobalConfig {
        jwt: JwtConfig {
            issuer: default_issuer(),
            ttl_seconds: default_ttl_seconds(),
        },
        max_body_bytes: default_max_body(),
    });

    Ok(Json(ConfigResponse {
        global,
        global_modify_index,
        channels,
    }))
}

/// POST /api/config
///
/// Writes the updated config to Consul KV using CAS to prevent concurrent
/// overwrites. Returns 409 Conflict if any CAS write fails.
async fn save_config(
    State(state): State<AppState>,
    Json(req): Json<SaveConfigRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Write global config.
    let cas_ok = state
        .consul
        .put_cas("gateway/config/global", &req.global, req.global_modify_index)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    if !cas_ok {
        return Err((
            StatusCode::CONFLICT,
            "Concurrent modification detected on gateway/config/global. Reload and retry.".to_string(),
        ));
    }

    // Write each channel.
    for ch in &req.channels {
        let key = format!("gateway/config/channels/{}", ch.id);
        let modify_index = req.channel_modify_indexes.get(&ch.id).copied().unwrap_or(0);

        let cas_ok = state
            .consul
            .put_cas(&key, ch, modify_index)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

        if !cas_ok {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "Concurrent modification detected on channel '{}'. Reload and retry.",
                    ch.id
                ),
            ));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/config/channels/:id
async fn delete_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let key = format!("gateway/config/channels/{}", id);
    state
        .consul
        .delete_key(&key)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
