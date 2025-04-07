use std::{
    collections::HashMap,
    pin::pin,
    time::{Duration, SystemTime},
};

use exponential_histogram::ExponentialHistogram;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::AsciiMetadataValue;

use crate::{
    aggregation::{
        AbsorbDistribution, Aggregation, Centroid, Histogram, OnlineTdigest, StatisticSet,
    },
    pipeline::{AggregatedMetricsMap, AggregationBatcher, DimensionedMeasurementsMap},
    proto::{
        self,
        goodmetrics::{metrics_client::MetricsClient, Datum, MetricsRequest},
    },
    types::{Dimension, Distribution, Measurement, Name, Observation},
};

use super::{EpochTime, StdError};

/// A downstream that sends metrics to a `goodmetricsd` or other goodmetrics grpc server.
pub struct GoodmetricsDownstream<TChannel> {
    client: MetricsClient<TChannel>,
    header: Option<(&'static str, AsciiMetadataValue)>,
    shared_dimensions: HashMap<String, proto::goodmetrics::Dimension>,
}

impl<TChannel> GoodmetricsDownstream<TChannel>
where
    TChannel: tonic::client::GrpcService<tonic::body::Body>,
    TChannel::Error: Into<StdError>,
    TChannel::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
    <TChannel::ResponseBody as http_body::Body>::Error: Into<StdError> + Send,
{
    /// Create a new goodmetrics sender from a grpc channel
    pub fn new(
        client: MetricsClient<TChannel>,
        header: Option<(&'static str, AsciiMetadataValue)>,
        shared_dimensions: impl IntoIterator<Item = (impl Into<String>, impl Into<Dimension>)>,
    ) -> Self {
        GoodmetricsDownstream {
            client,
            header,
            shared_dimensions: shared_dimensions
                .into_iter()
                .map(|(k, v)| (k.into(), v.into().into()))
                .collect(),
        }
    }

    /// Spawn this on a tokio runtime to send your metrics to your downstream receiver
    pub async fn send_batches_forever(self, receiver: mpsc::Receiver<Vec<Datum>>) {
        self.send_metrics_stream_forever(ReceiverStream::new(receiver))
            .await;
    }

    /// Spawn this on a tokio runtime to send your metrics to your downstream receiver
    pub async fn send_metrics_stream_forever(
        mut self,
        mut receiver: impl Stream<Item = Vec<Datum>> + Unpin,
    ) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            // Send as quickly as possible while there are more batches
            while let Some(batch) = pin!(&mut receiver).next().await {
                let result = self
                    .client
                    .send_metrics(self.request(MetricsRequest {
                        shared_dimensions: self.shared_dimensions.clone(),
                        metrics: batch,
                    }))
                    .await;
                match result {
                    Ok(success) => {
                        log::debug!("sent metrics: {success:?}");
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
        }
    }

    fn request<T>(&self, request: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(request);
        if let Some((header, value)) = self.header.as_ref() {
            request.metadata_mut().insert(*header, value.clone());
        }
        request
    }
}

/// The default mapping from in-memory representation to goodmetrics wire representation
pub struct GoodmetricsBatcher;

impl AggregationBatcher for GoodmetricsBatcher {
    type TBatch = Vec<Datum>;

    fn batch_aggregations(
        &mut self,
        now: SystemTime,
        covered_time: Duration,
        aggregations: &mut AggregatedMetricsMap,
    ) -> Self::TBatch {
        aggregations
            .drain()
            .flat_map(|(name, dimensioned_measurements)| {
                as_datums(name, now, covered_time, dimensioned_measurements)
            })
            .collect()
    }
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
                Dimension::Shared(s) => proto::goodmetrics::dimension::Value::String(
                    // Let's try to avoid cloning if this is the last place the string is shared
                    std::sync::Arc::<String>::try_unwrap(s).unwrap_or_else(|this| this.to_string()),
                ),
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
                        proto::goodmetrics::Histogram {
                            buckets: buckets.into_map(),
                        },
                    )
                }
                Aggregation::StatisticSet(statistic_set) => {
                    proto::goodmetrics::measurement::Value::StatisticSet(statistic_set.into())
                }
                Aggregation::Sum(sum) => proto::goodmetrics::measurement::Value::I64(sum.sum),
                Aggregation::TDigest(t_digest) => {
                    proto::goodmetrics::measurement::Value::Tdigest(t_digest.into())
                }
                Aggregation::ExponentialHistogram(histogram) => {
                    proto::goodmetrics::measurement::Value::Histogram(
                        proto::goodmetrics::Histogram {
                            buckets: make_histogram(histogram),
                        },
                    )
                }
            }),
        }
    }
}

fn make_histogram(eh: ExponentialHistogram) -> HashMap<i64, u64> {
    HashMap::from_iter(
        eh.value_counts()
            .map(|(value, count)| (value.round() as i64, count as u64)),
    )
}

impl From<Measurement> for proto::goodmetrics::Measurement {
    fn from(value: Measurement) -> Self {
        proto::goodmetrics::Measurement {
            value: Some(match value {
                Measurement::Observation(observation) => observation.into(),
                Measurement::Distribution(distribution) => distribution.into(),
                Measurement::Sum(sum) => proto::goodmetrics::measurement::Value::I64(sum),
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
        let mut histogram = Histogram::default();
        histogram.absorb(value);
        Self::Histogram(proto::goodmetrics::Histogram {
            buckets: histogram.into_map(),
        })
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
