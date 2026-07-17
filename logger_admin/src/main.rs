use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tokio::signal;

mod auth;
mod consul;
mod routes;

use auth::AdminToken;
use consul::ConsulClient;

// ─────────────────────────────────────────────────────────────────────────────
// CLI
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name    = "logger-admin",
    version = env!("CARGO_PKG_VERSION"),
    about   = "Stateless Logger Admin UI — manages channel config (routing + attestation) in Consul KV"
)]
struct Cli {
    /// TCP port to listen on.
    #[arg(long, env = "PORT", default_value = "8082")]
    port: u16,

    /// Consul HTTP API base URL.
    #[arg(long, env = "CONSUL_HTTP_ADDR", default_value = "http://localhost:8500")]
    consul: String,

    /// Bearer token required on all /api/* routes.
    /// MUST be set — the service panics on startup if this is missing.
    #[arg(long, env = "ADMIN_TOKEN")]
    admin_token: String,

    /// Unique node identifier for Consul service registration.
    /// Defaults to a freshly generated UUIDv4 so every instance is unique.
    #[arg(long, env = "NODE_ID", default_value = "")]
    node_id: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── CLI / env ─────────────────────────────────────────────────────────────
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

    // ── Resolve node identity ─────────────────────────────────────────────────
    let node_id = if cli.node_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        cli.node_id.clone()
    };

    let advertised_ip = resolve_advertised_ip()?;
    tracing::info!(
        node_id     = %node_id,
        ip          = %advertised_ip,
        port        = cli.port,
        consul      = %cli.consul,
        version     = env!("CARGO_PKG_VERSION"),
        "logger-admin starting"
    );

    // ── Shared state ──────────────────────────────────────────────────────────
    let consul = Arc::new(ConsulClient::new(&cli.consul));
    let token  = Arc::new(AdminToken::new(&cli.admin_token));

    // ── Seed Consul KV configuration if empty ─────────────────────────────────
    seed_consul_if_empty(&consul)
        .await
        .context("Seeding default Consul KV configuration")?;

    // ── Consul service registration ───────────────────────────────────────────
    consul
        .register_service(
            &node_id,
            "logger_admin",
            &advertised_ip,
            cli.port,
            &format!("http://{}:{}/health", advertised_ip, cli.port),
        )
        .await
        .context("Registering logger_admin with Consul")?;
    tracing::info!("Registered with Consul as 'logger_admin' (id: {})", node_id);

    // ── Router ────────────────────────────────────────────────────────────────
    let app = routes::build_router(consul.clone(), token);

    // ── Serve ─────────────────────────────────────────────────────────────────
    let addr = format!("0.0.0.0:{}", cli.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Binding to {addr}"))?;
    tracing::info!(addr = %addr, "logger-admin listening");

    // ── Graceful shutdown + Consul deregistration ─────────────────────────────
    let consul_shutdown = consul.clone();
    let node_id_shutdown = node_id.clone();

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait for SIGTERM or Ctrl-C.
            let _ = signal::ctrl_c().await;
            tracing::info!("Shutdown signal received — deregistering from Consul");
            if let Err(e) = consul_shutdown.deregister_service(&node_id_shutdown).await {
                tracing::warn!("Consul deregistration failed: {e}");
            } else {
                tracing::info!("Deregistered from Consul (id: {})", node_id_shutdown);
            }
        })
        .await
        .context("axum::serve error")
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Discover the machine's outbound IP by opening a UDP socket (no packet sent).
/// Mirrors the same technique used in `validation_worker`.
fn resolve_advertised_ip() -> anyhow::Result<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    let addr = socket.local_addr()?;
    Ok(addr.ip().to_string())
}

/// Query Consul KV at startup, and if the config prefix is empty, seed it with defaults.
async fn seed_consul_if_empty(consul: &ConsulClient) -> anyhow::Result<()> {
    tracing::info!("Checking if Consul KV configuration is empty...");
    let entries = consul.get_prefix("gateway/config/").await?;
    if entries.is_empty() {
        tracing::info!("Consul configuration is empty. Seeding defaults...");

        // 1. Seed global config
        let global_config = routes::GlobalConfig {
            jwt: routes::JwtConfig {
                issuer: "logger-gateway".to_string(),
                ttl_seconds: 900,
            },
            max_body_bytes: 2097152, // 2 MiB
        };
        let ok = consul.put_cas("gateway/config/global", &global_config, 0).await?;
        if !ok {
            tracing::warn!("Failed to seed global config (CAS conflict or already exists)");
        } else {
            tracing::info!("Successfully seeded gateway/config/global");
        }

        // 2. Seed example-app-dev channel
        let dev_channel = routes::ChannelConfig {
            id: "example-app-dev".to_string(),
            app_id: "dev.femrek.exampleapp.dev".to_string(),
            validate_attestation: false,
            require_apple_key_hash: false,
            google_project_number: None,
            apple_team_id: None,
        };
        let ok = consul.put_cas("gateway/config/channels/example-app-dev", &dev_channel, 0).await?;
        if !ok {
            tracing::warn!("Failed to seed dev channel (CAS conflict or already exists)");
        } else {
            tracing::info!("Successfully seeded gateway/config/channels/example-app-dev");
        }

        // 3. Seed example-app-prod channel
        let prod_channel = routes::ChannelConfig {
            id: "example-app-prod".to_string(),
            app_id: "dev.femrek.exampleapp".to_string(),
            validate_attestation: true,
            require_apple_key_hash: true,
            google_project_number: Some("1234567890".to_string()),
            apple_team_id: Some("STUBTEAMID".to_string()),
        };
        let ok = consul.put_cas("gateway/config/channels/example-app-prod", &prod_channel, 0).await?;
        if !ok {
            tracing::warn!("Failed to seed prod channel (CAS conflict or already exists)");
        } else {
            tracing::info!("Successfully seeded gateway/config/channels/example-app-prod");
        }
    } else {
        tracing::info!("Consul configuration already has existing keys. Skipping seeding.");
    }
    Ok(())
}
