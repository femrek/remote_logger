//! Production [`KafkaSink`] implementation backed by `librdkafka`.

use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::Duration;

use super::sink::{BoxFuture, KafkaSink, KafkaSinkError};

// ── Production Kafka sink ─────────────────────────────────────────────────────

/// Production [`KafkaSink`] implementation backed by an rdkafka
/// [`FutureProducer`]. Each `send` awaits the broker ACK before resolving,
/// providing a zero-data-loss delivery guarantee.
pub struct RdkafkaSink {
    producer: FutureProducer,
}

impl RdkafkaSink {
    /// Create a new sink connected to the given comma-separated `brokers` list.
    pub fn new(brokers: &str) -> Result<Self, rdkafka::error::KafkaError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("reconnect.backoff.ms", "10000")
            .set("reconnect.backoff.max.ms", "10000")
            .create()?;
        Ok(Self { producer })
    }
}

impl KafkaSink for RdkafkaSink {
    fn send<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> BoxFuture<'a, Result<(), KafkaSinkError>> {
        Box::pin(async move {
            let record = FutureRecord::to(topic).key(key).payload(payload);
            self.producer
                .send(record, Duration::from_secs(5))
                .await
                .map(|_| ())
                .map_err(|(e, _)| KafkaSinkError(e.to_string()))
        })
    }
}
