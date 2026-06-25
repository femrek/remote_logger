//! Async plumbing and the object-safe [`KafkaSink`] abstraction.
//!
//! Decoupling the handler from a concrete producer lets unit tests inject a
//! zero-network [`NoopSink`] while production code uses [`RdkafkaSink`].

use std::{future::Future, pin::Pin};

// ── Async plumbing ────────────────────────────────────────────────────────────

/// Object-safe boxed future used as the return type of [`KafkaSink::send`].
/// Avoids pulling in `async-trait` while remaining fully compatible with
/// `Arc<dyn KafkaSink>` trait objects.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ── Error type ────────────────────────────────────────────────────────────────

/// Delivery error returned by a [`KafkaSink`] implementation.
#[derive(Debug)]
pub struct KafkaSinkError(pub String);

impl std::fmt::Display for KafkaSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KafkaSinkError: {}", self.0)
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// An object-safe, async abstraction over a Kafka message producer.
///
/// Decoupling the handler from a concrete `FutureProducer` lets unit tests
/// inject a zero-network [`NoopSink`] while production code uses the real
/// [`RdkafkaSink`] backed by `librdkafka`.
pub trait KafkaSink: Send + Sync {
    fn send<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> BoxFuture<'a, Result<(), KafkaSinkError>>;
}
