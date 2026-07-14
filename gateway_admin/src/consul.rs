use anyhow::Context;
use base64::Engine;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Consul KV response types
// ─────────────────────────────────────────────────────────────────────────────

/// A single entry from the Consul KV `?recurse` endpoint.
#[derive(Deserialize, Debug, Clone)]
pub struct KvEntry {
    #[serde(rename = "Key")]
    pub key: String,
    /// Base64-encoded value; `None` if the key has no value.
    #[serde(rename = "Value")]
    pub value: Option<String>,
    #[serde(rename = "ModifyIndex")]
    pub modify_index: u64,
}

impl KvEntry {
    /// Decode the base64 value into raw bytes.
    pub fn decoded_value(&self) -> anyhow::Result<Vec<u8>> {
        let b64 = self.value.as_deref().unwrap_or("");
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .context("base64 decode of Consul KV value")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Consul service registration payload
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ServiceRegistration<'a> {
    #[serde(rename = "ID")]
    id: &'a str,
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "Address")]
    address: &'a str,
    #[serde(rename = "Port")]
    port: u16,
    #[serde(rename = "Check")]
    check: HealthCheck<'a>,
}

#[derive(Serialize)]
struct HealthCheck<'a> {
    #[serde(rename = "HTTP")]
    http: &'a str,
    #[serde(rename = "Interval")]
    interval: &'static str,
    #[serde(rename = "Timeout")]
    timeout: &'static str,
    #[serde(rename = "DeregisterCriticalServiceAfter")]
    deregister_critical_service_after: &'static str,
}

// ─────────────────────────────────────────────────────────────────────────────
// ConsulClient
// ─────────────────────────────────────────────────────────────────────────────

/// Thin async wrapper around the Consul HTTP API.
pub struct ConsulClient {
    base_url: String,
    client: reqwest::Client,
}

impl ConsulClient {
    pub fn new(consul_addr: &str) -> Self {
        Self {
            base_url: consul_addr.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    // ── KV ───────────────────────────────────────────────────────────────────

    /// Fetch all entries under `key_prefix` (recursive).
    /// Returns an empty `Vec` if the prefix does not exist (404).
    pub async fn get_prefix(&self, key_prefix: &str) -> anyhow::Result<Vec<KvEntry>> {
        let url = format!("{}/v1/kv/{}?recurse=true", self.base_url, key_prefix);
        let res = self.client.get(&url).send().await.context("GET Consul KV")?;

        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        res.json::<Vec<KvEntry>>()
            .await
            .context("parsing Consul KV list")
    }

    /// Write a JSON-serializable value to `key` using Check-And-Set (CAS).
    ///
    /// - `modify_index = 0` → create-only (fails if key already exists).
    /// - `modify_index = N` → update only if current ModifyIndex is still N.
    ///
    /// Returns `Ok(true)` on success, `Ok(false)` on CAS conflict (concurrent
    /// write detected).
    pub async fn put_cas<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        modify_index: u64,
    ) -> anyhow::Result<bool> {
        let body = serde_json::to_vec(value).context("serializing KV value to JSON")?;
        let url = format!("{}/v1/kv/{}?cas={}", self.base_url, key, modify_index);

        let res = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .context("PUT Consul KV")?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("Consul KV PUT failed ({}): {}", status, body);
        }

        // Consul returns literal `true` or `false` in the body.
        let success: bool = res.json().await.context("parsing CAS response")?;
        Ok(success)
    }

    /// Delete a single key from Consul KV.
    pub async fn delete_key(&self, key: &str) -> anyhow::Result<()> {
        let url = format!("{}/v1/kv/{}", self.base_url, key);
        let res = self
            .client
            .delete(&url)
            .send()
            .await
            .context("DELETE Consul KV")?;

        if !res.status().is_success() {
            anyhow::bail!("Consul KV DELETE failed ({})", res.status());
        }
        Ok(())
    }

    // ── Service registration ──────────────────────────────────────────────────

    /// Register this service instance with the local Consul agent.
    pub async fn register_service(
        &self,
        id: &str,
        name: &str,
        address: &str,
        port: u16,
        health_url: &str,
    ) -> anyhow::Result<()> {
        let payload = ServiceRegistration {
            id,
            name,
            address,
            port,
            check: HealthCheck {
                http: health_url,
                interval: "10s",
                timeout: "5s",
                deregister_critical_service_after: "1m",
            },
        };

        let url = format!("{}/v1/agent/service/register", self.base_url);
        let res = self
            .client
            .put(&url)
            .json(&payload)
            .send()
            .await
            .context("Consul service registration")?;

        if !res.status().is_success() {
            anyhow::bail!("Consul registration failed ({})", res.status());
        }
        Ok(())
    }

    /// Deregister a service instance from the local Consul agent.
    pub async fn deregister_service(&self, id: &str) -> anyhow::Result<()> {
        let url = format!("{}/v1/agent/service/deregister/{}", self.base_url, id);
        let res = self
            .client
            .put(&url)
            .send()
            .await
            .context("Consul service deregistration")?;

        if !res.status().is_success() {
            anyhow::bail!("Consul deregistration failed ({})", res.status());
        }
        Ok(())
    }
}
