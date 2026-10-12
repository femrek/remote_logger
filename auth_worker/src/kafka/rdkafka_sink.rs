//! Production [`KafkaSink`] implementation backed by `librdkafka`.

use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::{Duration, Instant};

use super::sink::{BoxFuture, KafkaSink, KafkaSinkError};
use crate::metrics::{KAFKA_PRODUCE_DURATION_SECONDS, KAFKA_PRODUCE_TOTAL};

pub struct RdkafkaSink {
    producer: FutureProducer,
}

impl RdkafkaSink {
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
            let start = Instant::now();
            let record = FutureRecord::to(topic).key(key).payload(payload);
            let result = self
                .producer
                .send(record, Duration::from_secs(5))
                .await
                .map(|_| ())
                .map_err(|(e, _)| KafkaSinkError(e.to_string()));

            let elapsed = start.elapsed().as_secs_f64();
            let result_label = if result.is_ok() { "ok" } else { "err" };

            KAFKA_PRODUCE_TOTAL
                .with_label_values(&[topic, result_label])
                .inc();
            KAFKA_PRODUCE_DURATION_SECONDS
                .with_label_values(&[topic])
                .observe(elapsed);

            result
        })
    }
}
