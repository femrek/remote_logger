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
/// Only `gateway/config/global` is read by the gateway now. Channel config
/// is owned by the validation workers (also read from Consul KV).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayConfig {
    pub global: GlobalConfig,
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
    /// Resolve the public key from the hardcoded environment variable `GATEWAY_PUBLIC_KEY`.
    ///
    /// # Errors
    /// Returns an error if the environment variable is not set at runtime.
    pub fn resolved_public_key(&self) -> anyhow::Result<String> {
        let key_path = std::env::var("GATEWAY_PUBLIC_KEY_PATH")
            .unwrap_or_else(|_| "./public_key.pem".to_string());
        let key = std::fs::read_to_string(&key_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read JWT public key from '{}' (set by GATEWAY_PUBLIC_KEY_PATH): {}",
                key_path, e
            )
        })?;
        let processed = key.replace("\\n", "\n");
        let trimmed = processed.trim();
        if trimmed.starts_with("-----BEGIN") {
            Ok(trimmed.to_string())
        } else {
            Ok(format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----", trimmed))
        }
    }
}


