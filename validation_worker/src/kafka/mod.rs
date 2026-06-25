//! Kafka infrastructure layer.
//!
//! Exposes:
//! - [`KafkaSink`] — the async, object-safe producer abstraction.
//! - [`KafkaSinkError`] / [`BoxFuture`] — supporting types.
//! - [`RdkafkaSink`] — the production `librdkafka`-backed implementation.

mod rdkafka_sink;
mod sink;

pub use rdkafka_sink::RdkafkaSink;
pub use sink::{BoxFuture, KafkaSink, KafkaSinkError};
