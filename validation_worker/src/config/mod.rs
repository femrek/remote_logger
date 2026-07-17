//! Worker channel configuration — loaded from Consul KV, hot-reloaded via
//! Consul blocking queries.
//!
//! The unified channel schema lives at `gateway/config/channels/<id>` in Consul
//! and is managed by `logger_admin`. Workers read the same keys as the gateway
//! but deserialize the full schema including attestation-specific fields.
//!
//! ## Hot-reload
//! `spawn_kv_watcher` runs a Consul blocking-query loop (identical in structure
//! to the gateway's KV watcher). Config swaps are atomic via `ArcSwap::store`.

use anyhow::Context;
use arc_swap::ArcSwap;
use serde::Deserialize;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Schema
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level wrapper holding all channel configs loaded from Consul KV.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub channels: Vec<WorkerChannelConfig>,
}

impl WorkerConfig {
    /// Find a channel by its unique `id`.
    pub fn find_channel_by_id(&self, id: &str) -> Option<&WorkerChannelConfig> {
        self.channels.iter().find(|ch| ch.id == id)
    }
}

/// Unified channel config — stored at `gateway/config/channels/<id>` in Consul.
///
/// The `id` and `app_id` fields are shared with the gateway.
/// All other fields are worker-specific attestation policy.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerChannelConfig {
    /// Unique channel identifier — matches the JWT `sub` claim.
    pub id: String,

    /// Application bundle identifier — used as the `app` JWT claim and
    /// passed to the attestation verifier.
    pub app_id: String,

    /// When `true`, the attestation token is cryptographically verified.
    /// When `false`, the token is accepted as-is (useful for dev/CI).
    #[serde(default)]
    pub validate_attestation: bool,

    /// Google Cloud Project Number for Android Play Integrity verification.
    /// Required when `validate_attestation: true` and platform is Android.
    pub google_project_number: Option<String>,

    /// Apple Developer Team ID for Apple App Attest verification.
    /// Required when `validate_attestation: true` and platform is iOS.
    pub apple_team_id: Option<String>,

    /// When `true`: iOS clients MUST supply `device_public_key` in the
    /// `/auth/exchange` body. The SHA-256 of the key is embedded in the
    /// refresh JWT as the `apk_hash` claim.
    #[serde(default)]
    pub require_apple_key_hash: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Consul KV structures
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ConsulKvEntry {
    #[serde(rename = "Key")]
    key: String,
    /// Base64-encoded value.
    #[serde(rename = "Value")]
    value: Option<String>,
    #[serde(rename = "ModifyIndex")]
    modify_index: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared config pointer
// ─────────────────────────────────────────────────────────────────────────────

/// A reference-counted, atomically-swappable config pointer.
///
/// `ArcSwap::store` is lock-free for writers and wait-free for readers.
/// Handler tasks call `shared.load()` on every request — they see either the
/// old or the new config, never a partial read.
pub type SharedWorkerConfig = Arc<ArcSwap<WorkerConfig>>;

// ─────────────────────────────────────────────────────────────────────────────
// Parse
// ─────────────────────────────────────────────────────────────────────────────

fn base64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("base64 decode failed")
}

/// Parse the raw Consul KV list response into a [`WorkerConfig`].
///
/// Processes every `gateway/config/channels/<id>` key and deserializes it as
/// a [`WorkerChannelConfig`]. Unknown keys are silently ignored.
fn parse_kv_entries(entries: Vec<ConsulKvEntry>) -> anyhow::Result<WorkerConfig> {
    let mut channels = Vec::new();

    for entry in entries {
        let raw_value = match &entry.value {
            Some(v) => v.clone(),
            None => continue,
        };

        if !entry.key.starts_with("gateway/config/channels/") {
            continue;
        }

        let decoded = base64_decode(&raw_value)
            .with_context(|| format!("base64-decoding Consul KV value for key '{}'", entry.key))?;

        match serde_json::from_slice::<WorkerChannelConfig>(&decoded) {
            Ok(ch) => channels.push(ch),
            Err(e) => {
                tracing::warn!(
                    key = %entry.key,
                    error = %e,
                    "Skipping channel entry: failed to parse as WorkerChannelConfig"
                );
            }
        }
    }

    if channels.is_empty() {
        tracing::warn!(
            "No channels found under 'gateway/config/channels/' in Consul KV. \
             Use logger_admin to configure channels."
        );
    }

    Ok(WorkerConfig { channels })
}

// ─────────────────────────────────────────────────────────────────────────────
// Load (initial, blocking)
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the current channel config from Consul KV and return it as a [`WorkerConfig`].
///
/// One-shot async call used at startup. `spawn_kv_watcher` takes over for
/// hot-reloading afterwards.
pub async fn load_config_from_consul(consul_addr: &str) -> anyhow::Result<WorkerConfig> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/kv/gateway/config/channels/?recurse=true", consul_addr);

    let res = client
        .get(&url)
        .send()
        .await
        .context("connecting to Consul KV for channel config")?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::warn!(
            "Consul KV prefix 'gateway/config/channels/' not found. \
             Starting with empty channel list — use logger_admin to configure channels."
        );
        return Ok(WorkerConfig { channels: vec![] });
    }

    let entries: Vec<ConsulKvEntry> = res
        .json()
        .await
        .context("parsing Consul KV channel list response")?;

    parse_kv_entries(entries)
}

/// Load the initial config from Consul KV and wrap it in a [`SharedWorkerConfig`].
///
/// Call this once at process startup before the Axum server starts accepting
/// connections.
pub async fn bootstrap(consul_addr: &str) -> anyhow::Result<SharedWorkerConfig> {
    let cfg = load_config_from_consul(consul_addr).await?;
    Ok(Arc::new(ArcSwap::from_pointee(cfg)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hot-reload watcher (Consul KV blocking query)
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn a background Tokio task that watches `gateway/config/channels/` in
/// Consul KV using HTTP blocking queries and atomically hot-reloads the channel
/// configuration.
///
/// ## Safety properties
/// - A failed reload logs the error and **leaves the current live config
///   untouched** — the worker continues serving with the last-known-good config.
/// - The `ArcSwap::store` call is atomic; no handler ever sees a partially-updated config.
pub async fn spawn_kv_watcher(
    consul_addr: String,
    shared: SharedWorkerConfig,
) -> anyhow::Result<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("building reqwest client for worker KV watcher");

        let base_url = format!(
            "{}/v1/kv/gateway/config/channels/?recurse=true",
            consul_addr
        );
        let mut last_index: u64 = 0;

        loop {
            let url = format!("{}&index={}&wait=30s", base_url, last_index);

            match client.get(&url).send().await {
                Ok(res) => {
                    let new_index = res
                        .headers()
                        .get("X-Consul-Index")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(last_index);

                    if new_index > last_index {
                        last_index = new_index;

                        if res.status() == reqwest::StatusCode::NOT_FOUND {
                            tracing::warn!(
                                "Consul KV prefix 'gateway/config/channels/' was deleted \
                                 — keeping current channel config live"
                            );
                            continue;
                        }

                        match res.json::<Vec<ConsulKvEntry>>().await {
                            Ok(entries) => match parse_kv_entries(entries) {
                                Ok(new_cfg) => {
                                    shared.store(Arc::new(new_cfg));
                                    tracing::info!(
                                        consul_index = last_index,
                                        "Worker channel config hot-reloaded from Consul KV"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Channel config parse failed — keeping current config live"
                                    );
                                }
                            },
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    "Failed to parse Consul KV channel response — keeping current config live"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Consul KV channel watcher request failed — retrying in 5s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });

    Ok(())
}
