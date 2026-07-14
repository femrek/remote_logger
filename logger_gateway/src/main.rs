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

use config::{SharedConfig, SystemConfig, bootstrap, spawn_kv_watcher};
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
    /// Consul HTTP API address used for both service discovery and KV config.
    #[arg(short = 'c', long, env = "CONSUL_HTTP_ADDR", default_value = "http://localhost:8500")]
    consul: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Background services
// ─────────────────────────────────────────────────────────────────────────────

/// Watches `gateway/config/` in Consul KV with a blocking query and
/// hot-reloads the routing table on every change.
struct ConsulKvWatcherService {
    consul_addr: String,
    shared: SharedConfig,
}

#[async_trait]
impl BackgroundService for ConsulKvWatcherService {
    async fn start(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        match spawn_kv_watcher(self.consul_addr.clone(), self.shared.clone()).await {
            Ok(()) => tracing::info!("Consul KV config watcher started"),
            Err(e) => {
                tracing::error!("Failed to start Consul KV watcher: {e}");
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

    // ── System config from env (bind address, metrics address, TLS) ───────────
    let system = SystemConfig::from_env();
    tracing::info!(
        bind_addr    = %system.bind_addr,
        metrics_addr = %system.metrics_addr,
        tls          = system.tls.is_some(),
        version      = env!("CARGO_PKG_VERSION"),
        consul       = %cli.consul,
        "logger-gateway starting"
    );

    // ── Prometheus metrics registration ───────────────────────────────────────
    metrics::register_all().context("Registering Prometheus metrics")?;

    // ── Initial config load from Consul KV ────────────────────────────────────
    // Bootstrap needs an async runtime for the initial Consul KV fetch.
    // We create a one-shot runtime here, fetch the config, and then drop it
    // before handing control to Pingora's own internal Tokio runtime via
    // `server.run_forever()`.
    let shared_config = {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Building bootstrap Tokio runtime")?;
        rt.block_on(async {
            bootstrap(&cli.consul)
                .await
                .with_context(|| format!("Loading initial config from Consul KV at '{}'", cli.consul))
        })?
    };

    // ── Build Pingora server ──────────────────────────────────────────────────
    let mut server = Server::new(None).context("Creating Pingora server")?;
    server.bootstrap();

    // ── Service Discovery ─────────────────────────────────────────────────────
    let lb = load_initial_workers(&cli.consul)
        .with_context(|| format!("Initializing Consul load balancer for '{}'", cli.consul))?;
    let discovery = Arc::new(ServiceDiscovery::new(lb));

    // ── Proxy service ─────────────────────────────────────────────────────────
    let gateway = LogGateway {
        config: shared_config.clone(),
        discovery: discovery.clone(),
    };

    let mut proxy_svc = http_proxy_service(&server.configuration, gateway);

    match &system.tls {
        Some(tls) => {
            // TLS termination at the Pingora listener.
            proxy_svc
                .add_tls(&system.bind_addr, &tls.cert_path, &tls.key_path)
                .with_context(|| {
                    format!(
                        "Setting up TLS on {} (cert: {}, key: {})",
                        system.bind_addr, tls.cert_path, tls.key_path
                    )
                })?;
            tracing::info!(addr = %system.bind_addr, "Proxy listener started (TLS)");
        }
        None => {
            proxy_svc.add_tcp(&system.bind_addr);
            tracing::info!(addr = %system.bind_addr, "Proxy listener started (plain HTTP)");
        }
    }

    server.add_service(proxy_svc);

    // ── Background: Consul KV config watcher ──────────────────────────────────
    server.add_service(background_service(
        "consul-kv-watcher",
        ConsulKvWatcherService {
            consul_addr: cli.consul.clone(),
            shared: shared_config,
        },
    ));

    // ── Background: Consul service discovery watcher ──────────────────────────
    struct DiscoveryWatcherService {
        consul_addr: String,
        discovery: Arc<ServiceDiscovery>,
    }

    #[async_trait]
    impl BackgroundService for DiscoveryWatcherService {
        async fn start(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
            if let Err(e) = spawn_discovery_watcher(self.consul_addr.clone(), self.discovery.clone()).await {
                tracing::error!("Failed to start discovery watcher: {e}");
            }
            let _ = shutdown.changed().await;
        }
    }

    server.add_service(background_service(
        "discovery-watcher",
        DiscoveryWatcherService {
            consul_addr: cli.consul.clone(),
            discovery,
        },
    ));

    // ── Background: Prometheus metrics endpoint ────────────────────────────────
    server.add_service(background_service(
        "metrics-server",
        MetricsService {
            addr: system.metrics_addr.clone(),
        },
    ));

    tracing::info!(
        addr = %system.metrics_addr,
        "Prometheus metrics available at http://{}/metrics",
        system.metrics_addr
    );

    // ── Run (blocks until SIGTERM / SIGINT) ───────────────────────────────────
    server.run_forever()
}
