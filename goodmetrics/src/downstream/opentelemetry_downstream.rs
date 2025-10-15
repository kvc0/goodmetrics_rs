use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    pin::pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use exponential_histogram::ExponentialHistogram;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::{AsciiMetadataKey, AsciiMetadataValue};

use crate::{
    aggregation::Histogram,
    pipeline::AggregatedMetricsMap,
    proto::opentelemetry::metrics::v1::{
        exponential_histogram_data_point::Buckets, ExponentialHistogramDataPoint,
    },
};
use crate::{
    aggregation::Sum,
    pipeline::AggregationBatcher,
    proto::opentelemetry::{metrics::v1::Gauge, resource::v1::Resource},
};
use crate::{
    aggregation::{bucket_10_below_2_sigfigs, Aggregation, StatisticSet},
    pipeline::{DimensionPosition, DimensionedMeasurementsMap},
    proto::opentelemetry::{
        self,
        collector::metrics::v1::{
            metrics_service_client::MetricsServiceClient, ExportMetricsServiceRequest,
        },
        common::v1::{any_value::Value, AnyValue, InstrumentationScope, KeyValue},
        metrics::v1::{
            AggregationTemporality, DataPointFlags, HistogramDataPoint, Metric, ResourceMetrics,
            ScopeMetrics,
        },
    },
    types::{Dimension, Name},
};

use super::{EpochTime, StdError};

/// Goodmetrics only records "delta" data as that word is understood by opentelemetry.
/// Goodmetrics records windows of aggregation_width and submits whatever was recorded
/// during each window. While there are some possibly-legitimate cases for something
/// marked as cumulative, goodmetrics is oriented instead toward high-cardinality
/// actions rather than system counters.
const THE_ACTUAL_TEMPORALITY: i32 = AggregationTemporality::Delta as i32;

/// Compatibility adapter downstream for OTLP. No dependency on opentelemetry code,
/// only their protos. All your measurements will be Delta temporality.
pub struct OpenTelemetryDownstream<TChannel> {
    client: MetricsServiceClient<TChannel>,
    header: Option<(AsciiMetadataKey, AsciiMetadataValue)>,
    shared_dimensions: Option<Vec<KeyValue>>,
}

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

impl<TChannel> OpenTelemetryDownstream<TChannel>
where
    TChannel: tonic::client::GrpcService<tonic::body::Body>,
    TChannel::Error: Into<StdError>,
    TChannel::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
    <TChannel::ResponseBody as http_body::Body>::Error: Into<StdError> + Send,
{
    /// Create a new opentelemetry metrics sender from a grpc channel
    pub fn new(
        client: MetricsServiceClient<TChannel>,
        header: Option<(impl Into<String>, AsciiMetadataValue)>,
    ) -> Self {
        OpenTelemetryDownstream {
            client,
            header: header.map(|(k, v)| (k.into().parse().expect("header name must be valid"), v)),
            shared_dimensions: None,
        }
    }

    /// Create a new opentelemetry metrics sender from a grpc channel, with a set of dimensions applied
    /// to all metrics sent through it.
    pub fn new_with_dimensions(
        client: MetricsServiceClient<TChannel>,
        header: Option<(impl Into<String>, AsciiMetadataValue)>,
        shared_dimensions: impl IntoIterator<Item = (impl Into<Name>, impl Into<Dimension>)>,
    ) -> Self {
        OpenTelemetryDownstream {
            client,
            header: header.map(|(k, v)| (k.into().parse().expect("header name must be valid"), v)),
            shared_dimensions: Some(as_otel_dimensions(
                shared_dimensions
                    .into_iter()
                    .map(|(n, d)| (n.into(), d.into()))
                    .collect::<DimensionPosition>(),
            )),
        }
    }

    /// Spawn this on a tokio runtime to send your metrics to your downstream receiver
    pub async fn send_batches_forever(self, receiver: mpsc::Receiver<Vec<Metric>>) {
        self.send_metrics_stream_forever(ReceiverStream::new(receiver))
            .await;
    }

    /// Spawn this on a tokio runtime to send your metrics to your downstream receiver
    pub async fn send_metrics_stream_forever(
        mut self,
        mut receiver: impl Stream<Item = Vec<Metric>> + Unpin,
    ) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            // Send as quickly as possible while there are more batches
            while let Some(batch) = pin!(&mut receiver).next().await {
                let result = self
                    .client
                    .export(self.request(ExportMetricsServiceRequest {
                        resource_metrics: vec![ResourceMetrics {
                            resource: self.shared_dimensions.as_ref().map(|dimensions| Resource {
                                attributes: dimensions.clone(),
                                dropped_attributes_count: 0,
                            }),
                            schema_url: "".to_string(),
                            scope_metrics: vec![ScopeMetrics {
                                scope: Some(InstrumentationScope {
                                    name: "goodmetrics".to_string(),
                                    version: VERSION.unwrap_or("unknown").to_string(),
                                }),
                                schema_url: "".to_string(),
                                metrics: batch,
                            }],
                        }],
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
            request.metadata_mut().insert(header.clone(), value.clone());
        }
        request
    }
}

/// The default mapping from in-memory representation to opentelemetry metrics wire representation
pub struct OpentelemetryBatcher;

impl AggregationBatcher for OpentelemetryBatcher {
    type TBatch = Vec<Metric>;

    fn batch_aggregations(
        &mut self,
        now: SystemTime,
        covered_time: Duration,
        aggregations: &mut AggregatedMetricsMap,
    ) -> Self::TBatch {
        aggregations
            .drain()
            .flat_map(|(name, dimensioned_measurements)| {
                as_metrics(name, now, covered_time, dimensioned_measurements)
            })
            .collect()
    }
}

fn as_metrics(
    name: Name,
    timestamp: SystemTime,
    duration: Duration,
    mut dimensioned_measurements: DimensionedMeasurementsMap,
) -> Vec<Metric> {
    dimensioned_measurements
        .drain()
        .flat_map(|(dimension_position, mut measurements)| {
            let otel_dimensions = as_otel_dimensions(dimension_position);
            measurements
                .drain()
                .flat_map(|(measurement_name, aggregation)| match aggregation {
                    Aggregation::ExponentialHistogram(eh) => {
                        vec![Metric {
                            name: format!("{name}_{measurement_name}"),
                            description: "".into(),
                            unit: "1".into(),
                            data: Some(
                                opentelemetry::metrics::v1::metric::Data::ExponentialHistogram(
                                    as_otel_exponential_histogram(
                                        eh,
                                        timestamp,
                                        duration,
                                        otel_dimensions.clone(),
                                    ),
                                ),
                            ),
                        }]
                    }
                    Aggregation::Histogram(h) => {
                        vec![Metric {
                            name: format!("{name}_{measurement_name}"),
                            data: Some(opentelemetry::metrics::v1::metric::Data::Histogram(
                                as_otel_histogram(h, timestamp, duration, otel_dimensions.clone()),
                            )),
                            description: "".into(),
                            unit: "1".into(),
                        }]
                    }
                    Aggregation::StatisticSet(s) => as_otel_statistic_set(
                        s,
                        &format!("{name}_{measurement_name}"),
                        timestamp,
                        duration,
                        &otel_dimensions,
                    ),
                    Aggregation::Sum(s) => vec![as_otel_sum(
                        s,
                        &format!("{name}_{measurement_name}"),
                        timestamp,
                        duration,
                        &otel_dimensions,
                    )],
                    Aggregation::TDigest(_) => {
                        unimplemented!("tdigest for opentelemetry is not implemented")
                    }
                })
                .collect::<Vec<Metric>>()
        })
        .collect()
}

fn as_otel_dimensions(dimension_position: DimensionPosition) -> Vec<KeyValue> {
    dimension_position
        .into_iter()
        .map(|(dimension, value)| KeyValue {
            key: dimension.into(),
            value: Some(value.into()),
        })
        .collect()
}

impl From<Dimension> for AnyValue {
    fn from(good_dimension: Dimension) -> Self {
        AnyValue {
            value: Some(match good_dimension {
                Dimension::Str(s) => Value::StringValue(s.into()),
                Dimension::String(s) => Value::StringValue(s),
                Dimension::Shared(s) => Value::StringValue(
                    // Let's try to avoid cloning if this is the last place the string is shared
                    std::sync::Arc::<String>::try_unwrap(s).unwrap_or_else(|this| this.to_string()),
                ),
                Dimension::Number(n) => Value::IntValue(n as i64),
                Dimension::Boolean(b) => Value::BoolValue(b),
            }),
        }
    }
}

fn as_otel_statistic_set(
    statistic_set: StatisticSet,
    full_measurement_name: &str,
    timestamp: SystemTime,
    duration: Duration,
    attributes: &[KeyValue],
) -> Vec<opentelemetry::metrics::v1::Metric> {
    let timestamp_nanos = timestamp
        .duration_since(UNIX_EPOCH)
        .expect("could not get system time")
        .as_nanos() as u64;
    let start_nanos = timestamp_nanos - duration.as_nanos() as u64;
    vec![
        statistic_set_gauge_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "min",
            statistic_set.min.into(),
        ),
        statistic_set_gauge_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "max",
            statistic_set.max.into(),
        ),
        statistic_set_counter_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "sum",
            statistic_set.sum.into(),
        ),
        statistic_set_counter_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "count",
            statistic_set.count.into(),
        ),
    ]
}

fn as_otel_sum(
    sum: Sum,
    full_measurement_name: &str,
    timestamp: SystemTime,
    duration: Duration,
    attributes: &[KeyValue],
) -> opentelemetry::metrics::v1::Metric {
    let timestamp_nanos = timestamp
        .duration_since(UNIX_EPOCH)
        .expect("could not get system time")
        .as_nanos() as u64;
    let start_nanos = timestamp_nanos - duration.as_nanos() as u64;

    Metric {
        name: full_measurement_name.to_string(),
        data: Some(opentelemetry::metrics::v1::metric::Data::Sum(
            opentelemetry::metrics::v1::Sum {
                aggregation_temporality: THE_ACTUAL_TEMPORALITY,
                // This resets every aggregation interval.
                // It might not play very nicely with prometheus, but goodmetrics does not limit
                // to the lowest common monitoring denominator.
                is_monotonic: true,
                data_points: vec![new_number_data_point(
                    timestamp_nanos,
                    start_nanos,
                    attributes,
                    sum.sum.into(),
                )],
            },
        )),
        description: "".into(),
        unit: "1".into(),
    }
}

impl From<i64> for opentelemetry::metrics::v1::number_data_point::Value {
    fn from(i: i64) -> Self {
        Self::AsInt(i)
    }
}

impl From<u64> for opentelemetry::metrics::v1::number_data_point::Value {
    fn from(i: u64) -> Self {
        Self::AsInt(i as i64)
    }
}

impl From<f64> for opentelemetry::metrics::v1::number_data_point::Value {
    fn from(i: f64) -> Self {
        Self::AsDouble(i)
    }
}

/// For numbers that sum up per reporting window
fn statistic_set_counter_component(
    full_measurement_name: &str,
    unix_nanos: u64,
    start_time_unix_nanos: u64,
    attributes: &[KeyValue],
    component: &str,
    value: opentelemetry::metrics::v1::number_data_point::Value,
) -> opentelemetry::metrics::v1::Metric {
    Metric {
        name: format!("{full_measurement_name}_{component}"),
        data: Some(opentelemetry::metrics::v1::metric::Data::Sum(
            opentelemetry::metrics::v1::Sum {
                aggregation_temporality: THE_ACTUAL_TEMPORALITY,
                // This resets every aggregation interval.
                // It might not play very nicely with prometheus, but goodmetrics does not limit
                // to the lowest common monitoring denominator.
                is_monotonic: true,
                data_points: vec![new_number_data_point(
                    unix_nanos,
                    start_time_unix_nanos,
                    attributes,
                    value,
                )],
            },
        )),
        description: "".into(),
        unit: "1".into(),
    }
}

/// For numbers that snapshot per reporting window
fn statistic_set_gauge_component(
    full_measurement_name: &str,
    unix_nanos: u64,
    start_time_unix_nanos: u64,
    attributes: &[KeyValue],
    component: &str,
    value: opentelemetry::metrics::v1::number_data_point::Value,
) -> opentelemetry::metrics::v1::Metric {
    Metric {
        name: format!("{full_measurement_name}_{component}"),
        data: Some(opentelemetry::metrics::v1::metric::Data::Gauge(Gauge {
            data_points: vec![new_number_data_point(
                unix_nanos,
                start_time_unix_nanos,
                attributes,
                value,
            )],
        })),
        description: "".into(),
        unit: "1".into(),
    }
}

fn new_number_data_point(
    unix_nanos: u64,
    start_time_unix_nanos: u64,
    attributes: &[KeyValue],
    value: opentelemetry::metrics::v1::number_data_point::Value,
) -> opentelemetry::metrics::v1::NumberDataPoint {
    opentelemetry::metrics::v1::NumberDataPoint {
        attributes: attributes.to_owned(),
        start_time_unix_nano: start_time_unix_nanos,
        time_unix_nano: unix_nanos,
        exemplars: vec![],
        flags: DataPointFlags::FlagNone as u32,
        value: Some(value),
    }
}

fn as_otel_histogram(
    histogram: Histogram,
    timestamp: SystemTime,
    duration: Duration,
    attributes: Vec<KeyValue>,
) -> opentelemetry::metrics::v1::Histogram {
    let timestamp_nanos = timestamp.nanos_since_epoch();
    let mut histogram = histogram.into_map();
    let bucket_values_count = histogram.values().sum();
    let bucket_values_min = *histogram.keys().min().unwrap_or(&0) as f64;
    let bucket_values_max = *histogram.keys().max().unwrap_or(&0) as f64;
    // These 3 data structures could be grown and cached for reuse
    // We want this in min heap order, sorted by bucket
    let mut histogram: BinaryHeap<Reverse<(i64, u64)>> = histogram.drain().map(Reverse).collect();
    let mut sorted_bounds: Vec<f64> = vec![];
    let mut sorted_counts: Vec<u64> = vec![];
    sorted_bounds.reserve(histogram.len());
    sorted_counts.reserve(histogram.len());
    while let Some(Reverse((bucket, count))) = histogram.pop() {
        let below = bucket_10_below_2_sigfigs(bucket);
        // Make sure the empty ranges have 0'd out counts so lightstep can math.
        // This probably won't hurt other histogram implementations either.
        // Goodmetrics histograms are sparse, but lightstep wants dense histograms. They happen to magically
        // handle flexible-sized ranges with 0 counts though, so this is about as sparse as we can make these.
        match sorted_bounds.last() {
            Some(last_bound) => {
                if *last_bound < below as f64 {
                    sorted_bounds.push(below as f64);
                    sorted_counts.push(0);
                }
            }
            None => {
                sorted_bounds.push(below as f64);
                sorted_counts.push(0);
            }
        };
        sorted_bounds.push(bucket as f64);
        sorted_counts.push(count);
    }

    // utterly obnoxious histogram definition by otlp metrics here. There's this magical implicit +inf bucket
    // that you have to provide a count for... with goodmetrics histograms we already know what the maximum
    // value bucket is.
    sorted_counts.push(0);

    opentelemetry::metrics::v1::Histogram {
        aggregation_temporality: THE_ACTUAL_TEMPORALITY,
        data_points: vec![HistogramDataPoint {
            attributes,
            start_time_unix_nano: timestamp_nanos - duration.as_nanos() as u64,
            time_unix_nano: timestamp_nanos,
            count: bucket_values_count,
            explicit_bounds: sorted_bounds,
            bucket_counts: sorted_counts,
            exemplars: vec![],                      // just no.
            flags: DataPointFlags::FlagNone as u32, // i don't send useless buckets
            min: bucket_values_min,                 // just use the histogram...
            max: bucket_values_max,                 // just use the histogram...
        }],
    }
}

fn as_otel_exponential_histogram(
    exponential_histogram: ExponentialHistogram,
    timestamp: SystemTime,
    duration: Duration,
    attributes: Vec<KeyValue>,
) -> opentelemetry::metrics::v1::ExponentialHistogram {
    let timestamp_nanos = timestamp.nanos_since_epoch();
    let count = exponential_histogram.count() as u64;
    let sum = if exponential_histogram.has_negatives() {
        0_f64
    } else {
        exponential_histogram.sum()
    };
    let min = exponential_histogram.min();
    let max = exponential_histogram.max();
    let scale = exponential_histogram.scale() as i32;
    let bucket_start_offset = exponential_histogram.bucket_start_offset() as i32;
    let (positives, negatives) = exponential_histogram.take_counts();

    opentelemetry::metrics::v1::ExponentialHistogram {
        aggregation_temporality: THE_ACTUAL_TEMPORALITY,
        data_points: vec![ExponentialHistogramDataPoint {
            attributes,
            start_time_unix_nano: timestamp_nanos - duration.as_nanos() as u64,
            time_unix_nano: timestamp_nanos,
            count,
            sum,
            exemplars: vec![],                      // just no.
            flags: DataPointFlags::FlagNone as u32, // i don't send useless buckets
            min,
            max,
            scale,
            zero_count: 0, // I don't do this yet
            positive: Some(Buckets {
                offset: bucket_start_offset,
                bucket_counts: positives.into_iter().map(|u| u as u64).collect(),
            }),
            negative: Some(Buckets {
                offset: bucket_start_offset,
                bucket_counts: negatives.into_iter().map(|u| u as u64).collect(),
            }),
        }],
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tokio::sync::mpsc;
    use tonic::metadata::AsciiMetadataValue;

    use crate::{
        downstream::{
            channel_connection::get_client,
            opentelemetry_downstream::{OpenTelemetryDownstream, OpentelemetryBatcher},
        },
        metrics::Metrics,
        pipeline::{Aggregator, DistributionMode, StreamSink},
        proto::opentelemetry::collector::metrics::v1::metrics_service_client::MetricsServiceClient,
    };

    #[test_log::test(tokio::test)]
    async fn downstream_is_runnable() {
        let (_sink, receiver) = StreamSink::<Box<Metrics>>::new();
        let aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
        let (batch_sender, batch_receiver) = mpsc::channel(128);

        let client = get_client("localhost:6379", || None, MetricsServiceClient::with_origin)
            .expect("I can make ");

        let downstream =
            OpenTelemetryDownstream::new(client, Option::<(&str, AsciiMetadataValue)>::None);

        let metrics_tasks = tokio::task::LocalSet::new();

        let aggregator_handle = metrics_tasks.spawn_local(aggregator.aggregate_metrics_forever(
            Duration::from_millis(10),
            batch_sender,
            OpentelemetryBatcher,
        ));
        let downstream_joiner = metrics_tasks
            .spawn_local(async move { downstream.send_batches_forever(batch_receiver).await });

        aggregator_handle.abort();
        downstream_joiner.abort();
        metrics_tasks.await;
    }
}
