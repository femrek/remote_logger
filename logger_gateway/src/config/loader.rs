use super::schema::GatewayConfig;
use anyhow::Context;
use arc_swap::ArcSwap;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{path::Path, sync::Arc};
use tokio::sync::mpsc;

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
// Load
// ─────────────────────────────────────────────────────────────────────────────

/// Parse and validate the YAML config file at `path`.
pub fn load_config(path: &str) -> anyhow::Result<GatewayConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file '{path}'"))?;
    serde_yaml::from_str(&raw)
        .with_context(|| format!("parsing YAML in '{path}'"))
}

/// Load the initial config and wrap it in a `SharedConfig`.
///
/// This is the single call that should happen at process startup before the
/// Pingora server starts accepting connections.
pub fn bootstrap(path: &str) -> anyhow::Result<SharedConfig> {
    let cfg = load_config(path)?;
    Ok(Arc::new(ArcSwap::from_pointee(cfg)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Hot-reload watcher
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn a background Tokio task that watches `config_path` for write events
/// and atomically hot-reloads the routing table.
///
/// ## Safety properties
/// - A failed reload logs the error and **leaves the current live config
///   untouched** — the gateway continues serving with the last-known-good config.
/// - The `ArcSwap::store` call is atomic; no Pingora worker ever sees a
///   partially-updated config.
///
/// ## Usage
/// Call this from inside a Pingora `BackgroundService::start` so it runs
/// within Pingora's Tokio runtime.
pub async fn spawn_watcher(config_path: String, shared: SharedConfig) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(32);

    // `RecommendedWatcher` picks the best OS backend (kqueue on macOS, inotify
    // on Linux). The closure bridges synchronous notify callbacks to an async
    // Tokio channel via `blocking_send`.
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
        // Keep the watcher alive for the lifetime of this task.
        let _watcher = watcher;

        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) if matches!(event.kind, EventKind::Modify(_)) => {
                    match load_config(&config_path) {
                        Ok(new_cfg) => {
                            shared.store(Arc::new(new_cfg));
                            tracing::info!(
                                path = %config_path,
                                "Gateway config hot-reloaded — zero connections dropped"
                            );
                        }
                        Err(e) => {
                            // IMPORTANT: do NOT swap on error — keep serving
                            // with the current config.
                            tracing::error!(
                                path = %config_path,
                                error = %e,
                                "Config reload failed — keeping current config live"
                            );
                        }
                    }
                }
                Ok(_) => {} // access/metadata events — ignore
                Err(e) => {
                    tracing::warn!(error = %e, "Filesystem watcher error");
                }
            }
        }

        tracing::warn!("Config file watcher task exited");
    });

    Ok(())
}
