use std::{
    collections::{hash_map, HashMap},
    sync::mpsc::Receiver,
    time::{Duration, SystemTime},
};

use futures_timer::Delay;

use crate::{
    pipeline::{
        aggregating_sink::DimensionedMeasurementsMap,
        aggregation::{
            online_tdigest::OnlineTdigest, statistic_set::StatisticSet, tdigest::Centroid,
            Aggregation,
        },
        AbsorbDistribution,
    },
    proto::{
        self,
        goodmetrics::{metrics_client::MetricsClient, Datum, MetricsRequest},
    },
    types::{Dimension, Distribution, Measurement, Name, Observation},
};

use super::{channel_connection::ChannelType, EpochTime};

/// A downstream that sends metrics to a `goodmetricsd` or other goodmetrics grpc server.
pub struct GoodmetricsDownstream {
    client: MetricsClient<ChannelType>,
    shared_dimensions: HashMap<String, proto::goodmetrics::Dimension>,
}

impl GoodmetricsDownstream {
    pub fn new(
        channel: ChannelType,
        shared_dimensions: HashMap<String, impl Into<Dimension>>,
    ) -> Self {
        let client: MetricsClient<ChannelType> = MetricsClient::new(channel);

        GoodmetricsDownstream {
            client,
            shared_dimensions: shared_dimensions
                .into_iter()
                .map(|(k, v)| (k, v.into().into()))
                .collect(),
        }
    }

    pub async fn send_batches_forever(&mut self, receiver: Receiver<Vec<Datum>>) {
        let interval = Duration::from_secs(1);
        loop {
            match receiver.try_recv() {
                Ok(batch) => {
                    let future = self.client.send_metrics(MetricsRequest {
                        shared_dimensions: self.shared_dimensions.clone(),
                        metrics: batch,
                    });
                    match future.await {
                        Ok(success) => {
                            log::info!("sent metrics: {success:?}");
                        }
                        Err(err) => {
                            if !err.metadata().is_empty() {
                                log::error!(
                                    "failed to send metrics: {err}. Metadata: {:?}",
                                    err.metadata()
                                );
                            }
                            log::error!("failed to send metrics: {err:?}")
                        }
                    };
                }
                Err(timeout) => {
                    log::debug!("no metrics activity: {timeout}");
                    Delay::new(interval).await;
                }
            }
        }
    }
}

pub fn create_preaggregated_goodmetrics_batch(
    timestamp: SystemTime,
    duration: Duration,
    batch: hash_map::Drain<'_, Name, DimensionedMeasurementsMap>,
) -> Vec<Datum> {
    batch
        .flat_map(|(name, dimensioned_measurements)| {
            as_datums(name, timestamp, duration, dimensioned_measurements)
        })
        .collect()
}

fn as_datums(
    name: Name,
    timestamp: SystemTime,
    _duration: Duration,
    mut dimensioned_measurements: DimensionedMeasurementsMap,
) -> Vec<Datum> {
    dimensioned_measurements
        .drain()
        .map(|(dimension_position, mut measurements)| Datum {
            metric: name.to_string(),
            unix_nanos: timestamp.nanos_since_epoch(),
            dimensions: dimension_position
                .into_iter()
                .map(|(name, dimension)| (name.into(), dimension.into()))
                .collect(),
            measurements: measurements
                .drain()
                .map(|(name, aggregation)| (name.into(), aggregation.into()))
                .collect(),
        })
        .collect()
}

impl From<Dimension> for proto::goodmetrics::Dimension {
    fn from(value: Dimension) -> Self {
        Self {
            value: Some(match value {
                Dimension::Str(s) => proto::goodmetrics::dimension::Value::String(s.to_string()),
                Dimension::String(s) => proto::goodmetrics::dimension::Value::String(s),
                Dimension::Number(n) => proto::goodmetrics::dimension::Value::Number(n),
                Dimension::Boolean(b) => proto::goodmetrics::dimension::Value::Boolean(b),
            }),
        }
    }
}

impl From<Aggregation> for proto::goodmetrics::Measurement {
    fn from(value: Aggregation) -> Self {
        Self {
            value: Some(match value {
                Aggregation::Histogram(buckets) => {
                    proto::goodmetrics::measurement::Value::Histogram(
                        proto::goodmetrics::Histogram { buckets },
                    )
                }
                Aggregation::StatisticSet(statistic_set) => {
                    proto::goodmetrics::measurement::Value::StatisticSet(statistic_set.into())
                }
                Aggregation::TDigest(t_digest) => {
                    proto::goodmetrics::measurement::Value::Tdigest(t_digest.into())
                }
            }),
        }
    }
}

impl From<Measurement> for proto::goodmetrics::Measurement {
    fn from(value: Measurement) -> Self {
        proto::goodmetrics::Measurement {
            value: Some(match value {
                Measurement::Observation(observation) => observation.into(),
                Measurement::Distribution(distribution) => distribution.into(),
            }),
        }
    }
}

impl From<Observation> for proto::goodmetrics::measurement::Value {
    fn from(value: Observation) -> Self {
        Self::StatisticSet(proto::goodmetrics::StatisticSet {
            minimum: (&value).into(),
            maximum: (&value).into(),
            samplesum: (&value).into(),
            samplecount: 1,
        })
    }
}

impl From<StatisticSet> for proto::goodmetrics::StatisticSet {
    fn from(value: StatisticSet) -> Self {
        Self {
            minimum: value.min as f64,
            maximum: value.max as f64,
            samplesum: value.sum as f64,
            samplecount: value.count,
        }
    }
}

impl From<Distribution> for proto::goodmetrics::measurement::Value {
    fn from(value: Distribution) -> Self {
        let mut map = HashMap::new();
        map.absorb(value);
        Self::Histogram(proto::goodmetrics::Histogram { buckets: map })
    }
}

impl From<OnlineTdigest> for proto::goodmetrics::TDigest {
    fn from(mut value: OnlineTdigest) -> Self {
        let mut v = value.reset_mut();
        Self {
            centroids: v.drain_centroids().map(|c| c.into()).collect(),
            sum: v.sum(),
            count: v.count() as u64,
            max: v.max(),
            min: v.min(),
        }
    }
}

impl From<Centroid> for proto::goodmetrics::t_digest::Centroid {
    fn from(value: Centroid) -> Self {
        Self {
            mean: value.mean(),
            weight: value.weight() as u64,
        }
    }
}
