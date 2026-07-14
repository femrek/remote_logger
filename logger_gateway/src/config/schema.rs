use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// System-level config (loaded from environment variables, NOT from Consul KV)
// ─────────────────────────────────────────────────────────────────────────────

/// Network-binding configuration resolved exclusively from environment variables.
///
/// These fields control how the Pingora proxy process binds to the OS — they
/// are deliberately kept out of Consul KV so that restarting the process is
/// required to change them (bind address changes require a new listener).
#[derive(Debug, Clone)]
pub struct SystemConfig {
    /// `host:port` the Pingora listener binds to.
    /// Env: `GATEWAY_BIND_ADDR` (default: `"0.0.0.0:8080"`)
    pub bind_addr: String,

    /// `host:port` for the Prometheus `/metrics` scrape endpoint.
    /// Env: `GATEWAY_METRICS_ADDR` (default: `"0.0.0.0:9090"`)
    pub metrics_addr: String,

    /// Optional TLS termination. `None` (TLS disabled) unless **both**
    /// `GATEWAY_TLS_CERT_PATH` and `GATEWAY_TLS_KEY_PATH` are set.
    pub tls: Option<TlsConfig>,
}

impl SystemConfig {
    /// Build `SystemConfig` by reading environment variables.
    /// Never fails — all fields have safe defaults.
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("GATEWAY_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

        let metrics_addr = std::env::var("GATEWAY_METRICS_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:9090".to_string());

        let tls = match (
            std::env::var("GATEWAY_TLS_CERT_PATH"),
            std::env::var("GATEWAY_TLS_KEY_PATH"),
        ) {
            (Ok(cert_path), Ok(key_path)) => Some(TlsConfig { cert_path, key_path }),
            _ => None,
        };

        Self { bind_addr, metrics_addr, tls }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamic config (loaded from Consul KV as JSON, hot-reloadable)
// ─────────────────────────────────────────────────────────────────────────────

/// Root structure for gateway configuration stored in Consul KV.
///
/// This is assembled from two Consul KV keys:
/// - `gateway/config/global`   → serializes to [`GlobalConfig`]
/// - `gateway/config/channels/<id>` → each serializes to [`ChannelConfig`]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayConfig {
    pub global: GlobalConfig,
    pub channels: Vec<ChannelConfig>,
}

impl GatewayConfig {
    /// Find the first channel whose `path_prefix` is a prefix of `path`.
    /// Channels are evaluated in declaration order; put more-specific prefixes first.
    pub fn find_channel_by_path(&self, path: &str) -> Option<&ChannelConfig> {
        self.channels
            .iter()
            .find(|ch| path.starts_with(&ch.path_prefix))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Global / gateway-wide settings
// ─────────────────────────────────────────────────────────────────────────────

/// Gateway-wide settings stored under Consul KV key `gateway/config/global`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalConfig {
    pub jwt: JwtConfig,

    /// Hard cap on ingest request body size (bytes). Default: 2 MiB.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
}

fn default_max_body_bytes() -> usize {
    2 * 1024 * 1024
}

// ─────────────────────────────────────────────────────────────────────────────
// TLS
// ─────────────────────────────────────────────────────────────────────────────

/// TLS certificate and key paths, resolved from `SystemConfig::from_env()`.
/// Never stored in Consul KV.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// JWT
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    /// Inline public key. **Development only** — prefer `public_key_env`.
    #[serde(default)]
    pub public_key: Option<String>,

    /// Name of the environment variable that holds the public key.
    /// Takes priority over `public_key` when both are set.
    #[serde(default)]
    pub public_key_env: Option<String>,

    /// Value of the `iss` (issuer) claim. Default: `"logger-gateway"`.
    #[serde(default = "default_issuer")]
    pub issuer: String,

    /// Access token validity window in seconds (reference only — minting
    /// lives in the validation workers). Default: 900 (15 min).
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            public_key: None,
            public_key_env: Some("GATEWAY_PUBLIC_KEY".to_string()),
            issuer: default_issuer(),
            ttl_seconds: default_ttl_seconds(),
        }
    }
}

fn default_issuer() -> String {
    "logger-gateway".to_string()
}

fn default_ttl_seconds() -> u64 {
    900
}

impl JwtConfig {
    /// Resolve the public key, preferring the env-var form over the inline value.
    ///
    /// # Errors
    /// Returns an error if neither `public_key` nor `public_key_env` is configured, or
    /// if the named environment variable is not set at runtime.
    pub fn resolved_public_key(&self) -> anyhow::Result<String> {
        if let Some(env_name) = &self.public_key_env {
            std::env::var(env_name).map_err(|_| {
                anyhow::anyhow!(
                    "JWT public key env var '{}' is not set. \
                     Set it with: export {}=<public_key>",
                    env_name,
                    env_name
                )
            })
        } else if let Some(public_key) = &self.public_key {
            Ok(public_key.clone())
        } else {
            Err(anyhow::anyhow!(
                "No JWT public key configured. \
                 Add `jwt.public_key` (dev) or `jwt.public_key_env` (production) to Consul KV key gateway/config/global"
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Channel (per-application routing entry)
// ─────────────────────────────────────────────────────────────────────────────

/// Routing and JWT validation configuration for one application channel.
///
/// Stored under Consul KV key `gateway/config/channels/<id>`.
///
/// Attestation policy (validate_attestation, google_project_number,
/// apple_team_id, require_apple_key_hash) has been moved to the validation
/// worker's `channels.yaml`. This struct covers only gateway concerns:
/// path routing, JWT subject/app matching, and upstream header injection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelConfig {
    /// Unique channel identifier, used as the JWT `sub` claim.
    pub id: String,

    /// URL path prefix that routes requests to this channel.
    /// Example: `"/v1/logs/example-app-prod"`.
    pub path_prefix: String,

    /// Application bundle identifier (e.g., `"dev.femrek.exampleapp"`).
    pub app_id: String,
}
