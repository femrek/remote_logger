//! Async plumbing and the object-safe [`KafkaSink`] abstraction.

use std::{future::Future, pin::Pin};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug)]
pub struct KafkaSinkError(pub String);

impl std::fmt::Display for KafkaSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KafkaSinkError: {}", self.0)
    }
}

pub trait KafkaSink: Send + Sync {
    fn send<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> BoxFuture<'a, Result<(), KafkaSinkError>>;
}
