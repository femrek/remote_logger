use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use clap::Parser;
use pingora::proxy::http_proxy_service;
use pingora::server::Server;
use pingora::services::background::{BackgroundService, background_service};

mod auth;
mod config;
mod discovery;
mod error;
mod metrics;
mod proxy;

use auth::{
    AppleAppAttestVerifier, GooglePlayIntegrityVerifier,
    PlatformDispatchVerifier,
};
use config::{SharedConfig, bootstrap, spawn_watcher};
use discovery::{ServiceDiscovery, load_initial_workers, spawn_discovery_watcher};
use proxy::LogGateway;

// ─────────────────────────────────────────────────────────────────────────────
// CLI
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name        = "logger-gateway",
    version     = env!("CARGO_PKG_VERSION"),
    about       = "Pingora-based multi-tenant mobile log ingestion gateway"
)]
struct Cli {
    /// Path to the YAML configuration file.
    /// The file is watched at runtime; saving it triggers a zero-downtime reload.
    #[arg(short, long, default_value = "config/gateway.yaml")]
    config: String,

    /// Path to the JSON file containing the list of worker backends.
    #[arg(short = 'w', long, default_value = "config/workers_list.json")]
    workers: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Background services
// ─────────────────────────────────────────────────────────────────────────────

/// Watches the config file and hot-reloads on changes.
struct ConfigWatcherService {
    config_path: String,
    shared: SharedConfig,
}

#[async_trait]
impl BackgroundService for ConfigWatcherService {
    async fn start(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        match spawn_watcher(self.config_path.clone(), self.shared.clone()).await {
            Ok(()) => tracing::info!("Config file watcher started"),
            Err(e) => {
                tracing::error!("Failed to start config watcher: {e}");
                return;
            }
        }
        // Hold the task alive until Pingora signals shutdown.
        let _ = shutdown.changed().await;
    }
}

/// Serves the Prometheus `/metrics` endpoint on a dedicated port.
struct MetricsService {
    addr: String,
}

#[async_trait]
impl BackgroundService for MetricsService {
    async fn start(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        tokio::select! {
            result = metrics::serve_metrics(&self.addr) => {
                if let Err(e) = result {
                    tracing::error!("Metrics server error: {e}");
                }
            }
            _ = shutdown.changed() => {
                tracing::info!("Metrics server shutting down");
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Attestation verifier construction
// ─────────────────────────────────────────────────────────────────────────────

/// Build the platform-dispatching attestation verifier.
///
/// In production, replace the stubs with real verifier instances configured
/// from secrets / environment variables (project numbers, bundle IDs, etc.).
///
/// The `NoopVerifier` branch is never reached at runtime for channels that
/// have `validate_attestation: true` because the channel config is checked
/// inside `handle_exchange`. These stubs are always-pass by design for the PoC.
fn build_verifier() -> Arc<dyn auth::AttestationVerifier> {
    Arc::new(PlatformDispatchVerifier {
        android: Box::new(GooglePlayIntegrityVerifier),
        ios: Box::new(AppleAppAttestVerifier),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    // ── CLI args ──────────────────────────────────────────────────────────────
    let cli = Cli::parse();

    // ── Tracing ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config  = %cli.config,
        "logger-gateway starting"
    );

    // ── Prometheus metrics registration ───────────────────────────────────────
    metrics::register_all().context("Registering Prometheus metrics")?;

    // ── Initial config load ───────────────────────────────────────────────────
    let shared_config = bootstrap(&cli.config)
        .with_context(|| format!("Loading config from '{}'", cli.config))?;

    let cfg_snapshot = shared_config.load();
    let bind_addr = cfg_snapshot.gateway.bind_addr.clone();
    let metrics_addr = cfg_snapshot.gateway.metrics_addr.clone();
    let tls_config = cfg_snapshot.gateway.tls.clone();
    drop(cfg_snapshot); // release the guard

    // ── Build Pingora server ──────────────────────────────────────────────────
    // `Server::new(None)` uses Pingora's default configuration.
    // Pass `Some(Opt { ... })` to customise worker threads, daemon mode, etc.
    let mut server = Server::new(None).context("Creating Pingora server")?;
    server.bootstrap();

    // ── Service Discovery ─────────────────────────────────────────────────────
    let lb = load_initial_workers(&cli.workers)
        .with_context(|| format!("Loading initial workers from '{}'", cli.workers))?;
    let discovery = Arc::new(ServiceDiscovery::new(lb));

    // ── Proxy service ─────────────────────────────────────────────────────────
    let gateway = LogGateway {
        config: shared_config.clone(),
        verifier: build_verifier(),
        discovery: discovery.clone(),
    };

    let mut proxy_svc = http_proxy_service(&server.configuration, gateway);

    match &tls_config {
        Some(tls) => {
            // TLS termination at the Pingora listener.
            // Requires the `openssl` or `boringssl` feature flag.
            proxy_svc
                .add_tls(&bind_addr, &tls.cert_path, &tls.key_path)
                .with_context(|| {
                    format!(
                        "Setting up TLS on {bind_addr} (cert: {}, key: {})",
                        tls.cert_path, tls.key_path
                    )
                })?;
            tracing::info!(addr = %bind_addr, "Proxy listener started (TLS)");
        }
        None => {
            proxy_svc.add_tcp(&bind_addr);
            tracing::info!(addr = %bind_addr, "Proxy listener started (plain HTTP)");
        }
    }

    server.add_service(proxy_svc);

    // ── Background: config hot-reload watcher ─────────────────────────────────
    server.add_service(background_service(
        "config-watcher",
        ConfigWatcherService {
            config_path: cli.config.clone(),
            shared: shared_config,
        },
    ));

    // ── Background: discovery watcher ─────────────────────────────────────────
    struct DiscoveryWatcherService {
        path: String,
        discovery: Arc<ServiceDiscovery>,
    }

    #[async_trait]
    impl BackgroundService for DiscoveryWatcherService {
        async fn start(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
            if let Err(e) = spawn_discovery_watcher(self.path.clone(), self.discovery.clone()).await {
                tracing::error!("Failed to start discovery watcher: {e}");
            }
            let _ = shutdown.changed().await;
        }
    }

    server.add_service(background_service(
        "discovery-watcher",
        DiscoveryWatcherService {
            path: cli.workers.clone(),
            discovery,
        },
    ));

    // ── Background: Prometheus metrics endpoint ────────────────────────────────
    server.add_service(background_service(
        "metrics-server",
        MetricsService {
            addr: metrics_addr.clone(),
        },
    ));

    tracing::info!(addr = %metrics_addr, "Prometheus metrics available at http://{}/metrics", metrics_addr);

    // ── Run (blocks until SIGTERM / SIGINT) ───────────────────────────────────
    server.run_forever();
}
