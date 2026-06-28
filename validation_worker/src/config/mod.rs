//! Worker channel configuration — attestation policy per application channel.
//!
//! This module is analogous to the gateway's `config` module but covers only
//! worker-specific concerns: attestation flags, vendor IDs, and crypto policy.
//! Path routing and JWT subject matching remain in the gateway's `gateway.yaml`.

use anyhow::Context;
use arc_swap::ArcSwap;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::{path::Path, sync::Arc};
use tokio::sync::mpsc;

// ─────────────────────────────────────────────────────────────────────────────
// Schema
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level structure that maps 1-to-1 with `channels.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    pub channels: Vec<WorkerChannelConfig>,
}

impl WorkerConfig {
    /// Find a channel by its unique `id`.
    pub fn find_channel_by_id(&self, id: &str) -> Option<&WorkerChannelConfig> {
        self.channels.iter().find(|ch| ch.id == id)
    }
}

/// Attestation policy for one application channel.
///
/// The `id` and `app_id` fields mirror those in `gateway.yaml` — they are the
/// link between the two config files. All other fields are worker-specific.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerChannelConfig {
    /// Unique channel identifier — must match the gateway channel `id`.
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
    /// refresh JWT as the `apk_hash` claim, enabling stateless verification
    /// of future token refreshes on any worker replica.
    ///
    /// When `false`: `device_public_key` is ignored and `apk_hash` is absent.
    #[serde(default)]
    pub require_apple_key_hash: bool,
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
// Load
// ─────────────────────────────────────────────────────────────────────────────

/// Parse and validate the YAML config file at `path`.
pub fn load_config(path: &str) -> anyhow::Result<WorkerConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading worker config file '{path}'"))?;
    serde_yaml::from_str(&raw)
        .with_context(|| format!("parsing YAML in '{path}'"))
}

/// Load the initial config and wrap it in a `SharedWorkerConfig`.
///
/// Call this once at process startup before the Axum server starts accepting
/// connections.
pub fn bootstrap(path: &str) -> anyhow::Result<SharedWorkerConfig> {
    let cfg = load_config(path)?;
    Ok(Arc::new(ArcSwap::from_pointee(cfg)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hot-reload watcher
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn a background Tokio task that watches `config_path` for write events
/// and atomically hot-reloads the channel configuration.
///
/// ## Safety properties
/// - A failed reload logs the error and **leaves the current live config
///   untouched** — the worker continues serving with the last-known-good config.
/// - The `ArcSwap::store` call is atomic; no handler ever sees a partially-updated config.
pub async fn spawn_watcher(config_path: String, shared: SharedWorkerConfig) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(32);

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        NotifyConfig::default(),
    )
    .context("creating filesystem watcher")?;

    watcher
        .watch(Path::new(&config_path), RecursiveMode::NonRecursive)
        .with_context(|| format!("watching '{config_path}'"))?;

    tokio::spawn(async move {
        let _watcher = watcher;

        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) if matches!(event.kind, EventKind::Modify(_)) => {
                    match load_config(&config_path) {
                        Ok(new_cfg) => {
                            shared.store(Arc::new(new_cfg));
                            tracing::info!(
                                path = %config_path,
                                "Worker channel config hot-reloaded"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                path = %config_path,
                                error = %e,
                                "Channel config reload failed — keeping current config live"
                            );
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Filesystem watcher error");
                }
            }
        }

        tracing::warn!("Worker config file watcher task exited");
    });

    Ok(())
}
