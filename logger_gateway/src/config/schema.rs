use serde::Deserialize;

// ─────────────────────────────────────────────────────────────────────────────
// Top-level config
// ─────────────────────────────────────────────────────────────────────────────

/// Root structure that maps 1-to-1 with `gateway.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub gateway: GlobalConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalConfig {
    /// `host:port` the Pingora listener binds to (e.g. `"0.0.0.0:8080"`).
    pub bind_addr: String,

    /// Optional TLS termination. When absent the listener runs plain HTTP.
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    pub jwt: JwtConfig,

    /// Hard cap on ingest request body size (bytes). Default: 2 MiB.
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,

    /// `host:port` for the Prometheus `/metrics` scrape endpoint. Default: `0.0.0.0:9090`.
    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,
}

fn default_max_body_bytes() -> usize {
    2 * 1024 * 1024
}
fn default_metrics_addr() -> String {
    "0.0.0.0:9090".to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// TLS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// JWT
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
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
}

fn default_issuer() -> String {
    "logger-gateway".to_string()
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
                 Add `jwt.public_key` (dev) or `jwt.public_key_env` (production) to gateway.yaml"
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Channel (per-application routing entry)
// ─────────────────────────────────────────────────────────────────────────────

/// Routing and JWT validation configuration for one application channel.
///
/// Attestation policy (validate_attestation, google_project_number,
/// apple_team_id, require_apple_key_hash) has been moved to the validation
/// worker's `channels.yaml`. This struct now covers only gateway concerns:
/// path routing, JWT subject/app matching, and upstream header injection.
#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    /// Unique channel identifier, used as the JWT `sub` claim.
    pub id: String,

    /// URL path prefix that routes requests to this channel.
    /// Example: `"/v1/logs/example-app-prod"`.
    pub path_prefix: String,

    /// Application bundle identifier (e.g., `"dev.femrek.exampleapp"`).
    pub app_id: String,
}
