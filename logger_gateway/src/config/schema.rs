use serde::Deserialize;
use std::collections::HashMap;

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

    /// Find a channel by its unique `id`.
    pub fn find_channel_by_id(&self, id: &str) -> Option<&ChannelConfig> {
        self.channels.iter().find(|ch| ch.id == id)
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
    /// Inline signing secret. **Development only** — prefer `secret_env`.
    #[serde(default)]
    pub secret: Option<String>,

    /// Name of the environment variable that holds the signing secret.
    /// Takes priority over `secret` when both are set.
    #[serde(default)]
    pub secret_env: Option<String>,

    /// Session token lifetime in seconds. Default: 300 (5 minutes).
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: u64,

    /// Value of the `iss` (issuer) claim. Default: `"logger-gateway"`.
    #[serde(default = "default_issuer")]
    pub issuer: String,
}

fn default_ttl_seconds() -> u64 {
    300
}
fn default_issuer() -> String {
    "logger-gateway".to_string()
}

impl JwtConfig {
    /// Resolve the signing secret, preferring the env-var form over the inline value.
    ///
    /// # Errors
    /// Returns an error if neither `secret` nor `secret_env` is configured, or
    /// if the named environment variable is not set at runtime.
    pub fn resolved_secret(&self) -> anyhow::Result<String> {
        if let Some(env_name) = &self.secret_env {
            std::env::var(env_name).map_err(|_| {
                anyhow::anyhow!(
                    "JWT secret env var '{}' is not set. \
                     Set it with: export {}=<secret>",
                    env_name,
                    env_name
                )
            })
        } else if let Some(secret) = &self.secret {
            Ok(secret.clone())
        } else {
            Err(anyhow::anyhow!(
                "No JWT secret configured. \
                 Add `jwt.secret` (dev) or `jwt.secret_env` (production) to gateway.yaml"
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Channel (per-application routing entry)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    /// Unique channel identifier, used as the JWT `sub` claim.
    pub id: String,

    /// URL path prefix that routes requests to this channel.
    /// Example: `"/v1/logs/example-app-prod"`.
    pub path_prefix: String,

    /// Application bundle identifier (e.g., `"dev.femrek.exampleapp"`).
    pub app_id: String,

    /// Optional Google Cloud Project Number for Android Play Integrity verification.
    pub google_project_number: Option<String>,

    /// Optional Apple Developer Team ID for Apple App Attest verification.
    pub apple_team_id: Option<String>,

    /// When `true` the `/auth/exchange` endpoint requires a valid Google Play
    /// Integrity or Apple App Attest token before issuing a gateway JWT.
    pub validate_attestation: bool,

    /// Key-value pairs injected as HTTP headers into every upstream request.
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

