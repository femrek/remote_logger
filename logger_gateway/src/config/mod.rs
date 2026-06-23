pub mod loader;
pub mod schema;

// Convenience re-exports used by handler.rs and main.rs.
// The full module tree is pub, so callers can also use
// `config::schema::BackendConfig`, `config::loader::load_config`, etc.
pub use loader::{SharedConfig, bootstrap, spawn_watcher};
pub use schema::{GatewayConfig, JwtConfig};
