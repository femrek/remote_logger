pub mod loader;
pub mod schema;

// Convenience re-exports used by handler.rs and main.rs.
// The full module tree is pub, so callers can also use
// `config::schema::GatewayConfig`, `config::loader::SharedConfig`, etc.
pub use loader::{SharedConfig, bootstrap, spawn_kv_watcher};
pub use schema::{JwtConfig, SystemConfig};
