use super::schema::{GatewayConfig, GlobalConfig};
use anyhow::Context;
use arc_swap::ArcSwap;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// A reference-counted, atomically-swappable config pointer.
///
/// `ArcSwap::store` is lock-free for writers and wait-free for readers.
/// Pingora worker threads call `shared.load()` on every request — they see
/// either the old or the new config, never a partial read.
pub type SharedConfig = Arc<ArcSwap<GatewayConfig>>;

// ─────────────────────────────────────────────────────────────────────────────
// Consul KV structures
// ─────────────────────────────────────────────────────────────────────────────

/// A single Consul KV entry as returned by the `/v1/kv/?recurse` endpoint.
#[derive(serde::Deserialize, Debug)]
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
// Parse
// ─────────────────────────────────────────────────────────────────────────────

/// Parse the raw Consul KV list response into a [`GatewayConfig`].
///
/// Expected key:
/// - `gateway/config/global` → JSON [`GlobalConfig`]
///
/// Channel configs are no longer read by the gateway — they are owned
/// exclusively by the validation workers via their own Consul KV watcher.
///
/// The gateway **never writes** to Consul KV — seeding is the
/// responsibility of `logger_admin`.
fn parse_kv_entries(entries: Vec<ConsulKvEntry>) -> anyhow::Result<GatewayConfig> {
    let mut global: Option<GlobalConfig> = None;

    for entry in entries {
        let raw_value = match &entry.value {
            Some(v) => v.clone(),
            None => continue,
        };

        // Consul stores values as base64 — decode first.
        let decoded = base64_decode(&raw_value)
            .with_context(|| format!("base64-decoding Consul KV value for key '{}'", entry.key))?;

        if entry.key == "gateway/config/global" {
            global = Some(
                serde_json::from_slice(&decoded)
                    .with_context(|| "parsing gateway/config/global as JSON GlobalConfig")?,
            );
        }
        // All other keys (channels, etc.) are intentionally ignored by the gateway.
    }

    let global = match global {
        Some(g) => g,
        None => {
            tracing::warn!(
                "Consul KV key 'gateway/config/global' is missing. \
                 Using built-in defaults. Start logger_admin to seed the config."
            );
            GlobalConfig {
                jwt: Default::default(),
                max_body_bytes: 2 * 1024 * 1024,
            }
        }
    };

    Ok(GatewayConfig { global })
}

fn base64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .context("base64 decode failed")
}

// ─────────────────────────────────────────────────────────────────────────────
// Load (initial, blocking)
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the current config from Consul KV and return it as a [`GatewayConfig`].
///
/// This is a one-shot synchronous-style async call — used once at startup.
/// The watcher (`spawn_kv_watcher`) takes over for hot-reloading afterwards.
pub async fn load_config_from_consul(consul_addr: &str) -> anyhow::Result<GatewayConfig> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/kv/gateway/config/?recurse=true", consul_addr);

    let res = client
        .get(&url)
        .send()
        .await
        .context("connecting to Consul KV")?;

    if res.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::warn!(
            "Consul KV prefix 'gateway/config/' not found. \
             Starting with empty config — use logger_admin to seed the config."
        );
        return Ok(GatewayConfig {
            global: GlobalConfig {
                jwt: Default::default(),
                max_body_bytes: 2 * 1024 * 1024,
            },
        });
    }

    let entries: Vec<ConsulKvEntry> = res
        .json()
        .await
        .context("parsing Consul KV list response")?;

    parse_kv_entries(entries)
}

/// Load the initial config from Consul KV and wrap it in a [`SharedConfig`].
///
/// This is the single call that should happen at process startup before the
/// Pingora server starts accepting connections.
pub async fn bootstrap(consul_addr: &str) -> anyhow::Result<SharedConfig> {
    let cfg = load_config_from_consul(consul_addr).await?;
    Ok(Arc::new(ArcSwap::from_pointee(cfg)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hot-reload watcher (Consul KV blocking query)
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn a background Tokio task that watches the `gateway/config/` prefix in
/// Consul KV using HTTP blocking queries and atomically hot-reloads the config.
///
/// ## Safety properties
/// - A failed reload logs the error and **leaves the current live config
///   untouched** — the gateway continues serving with the last-known-good config.
/// - The `ArcSwap::store` call is atomic; no Pingora worker ever sees a
///   partially-updated config.
/// - The gateway is strictly **read-only** with respect to Consul KV.
///
/// ## Usage
/// Call this from inside a Pingora `BackgroundService::start` so it runs
/// within Pingora's Tokio runtime.
pub async fn spawn_kv_watcher(consul_addr: String, shared: SharedConfig) -> anyhow::Result<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            // The blocking query blocks for up to 30s server-side — give it
            // a generous client-side timeout.
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("building reqwest client for KV watcher");

        let base_url = format!("{}/v1/kv/gateway/config/?recurse=true", consul_addr);
        let mut last_index: u64 = 0;

        loop {
            // Consul blocking query: blocks until the modify-index changes or
            // `wait` expires, whichever comes first.
            let url = format!("{}&index={}&wait=30s", base_url, last_index);

            match client.get(&url).send().await {
                Ok(res) => {
                    // Extract the new X-Consul-Index header first.
                    let new_index = res
                        .headers()
                        .get("X-Consul-Index")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(last_index);

                    // Only reload if the index actually advanced.
                    if new_index > last_index {
                        last_index = new_index;

                        if res.status() == reqwest::StatusCode::NOT_FOUND {
                            tracing::warn!(
                                "Consul KV prefix 'gateway/config/' was deleted — keeping current config live"
                            );
                            continue;
                        }

                        match res.json::<Vec<ConsulKvEntry>>().await {
                            Ok(entries) => match parse_kv_entries(entries) {
                                Ok(new_cfg) => {
                                    shared.store(Arc::new(new_cfg));
                                    tracing::info!(
                                        consul_index = last_index,
                                        "Gateway config hot-reloaded from Consul KV — zero connections dropped"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Config parse failed — keeping current config live"
                                    );
                                }
                            },
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    "Failed to parse Consul KV response — keeping current config live"
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Consul KV watcher request failed — retrying in 5s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });

    Ok(())
}
