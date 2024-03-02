use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::sync::mpsc;

use crate::{pipeline::{aggregation::{exponential_histogram::ExponentialHistogram, histogram::Histogram}, aggregator::AggregatedMetricsMap}, proto::opentelemetry::metrics::v1::{exponential_histogram_data_point::Buckets, ExponentialHistogramDataPoint}};
use crate::proto::opentelemetry::resource::v1::Resource;
use crate::{
    pipeline::{
        aggregation::{
            bucket::bucket_10_below_2_sigfigs, statistic_set::StatisticSet, Aggregation,
        },
        aggregator::{DimensionPosition, DimensionedMeasurementsMap},
    },
    proto::opentelemetry::{
        self,
        collector::metrics::v1::{
            metrics_service_client::MetricsServiceClient, ExportMetricsServiceRequest,
        },
        common::v1::{any_value::Value, AnyValue, InstrumentationScope, KeyValue},
        metrics::v1::{
            AggregationTemporality, DataPointFlags, HistogramDataPoint, Metric, ResourceMetrics,
            ScopeMetrics, Sum,
        },
    },
    types::{Dimension, Name},
};

use super::{EpochTime, StdError};

// anything other than Delta is bugged by design. So yeah, opentelemetry metrics spec is bugged by design.
const THE_ONLY_SANE_TEMPORALITY: i32 = AggregationTemporality::Delta as i32;

/// Compatibility adapter downstream for OTLP. No dependency on opentelemetry code,
/// only their protos. All your measurements will be Delta temporality.
pub struct OpenTelemetryDownstream<TChannel> {
    client: MetricsServiceClient<TChannel>,
    shared_dimensions: Option<Vec<KeyValue>>,
}

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

impl<TChannel> OpenTelemetryDownstream<TChannel>
where
    TChannel: tonic::client::GrpcService<tonic::body::BoxBody>,
    TChannel::Error: Into<StdError>,
    TChannel::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
    <TChannel::ResponseBody as http_body::Body>::Error: Into<StdError> + Send,
{
    pub fn new(channel: TChannel, shared_dimensions: Option<DimensionPosition>) -> Self {
        let client: MetricsServiceClient<TChannel> = MetricsServiceClient::new(channel);

        OpenTelemetryDownstream {
            client,
            shared_dimensions: shared_dimensions.map(as_otel_dimensions),
        }
    }

    pub async fn send_batches_forever(mut self, mut receiver: mpsc::Receiver<Vec<Metric>>) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            // Send as quickly as possible while there are more batches
            while let Some(batch) = receiver.recv().await {
                let result = self
                    .client
                    .export(ExportMetricsServiceRequest {
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
                    })
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
}

pub fn create_preaggregated_opentelemetry_batch(
    timestamp: SystemTime,
    duration: Duration,
    batch: &mut AggregatedMetricsMap,
) -> Vec<Metric> {
    batch
        .drain()
        .flat_map(|(name, dimensioned_measurements)| {
            as_metrics(name, timestamp, duration, dimensioned_measurements)
        })
        .collect()
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
                            data: Some(opentelemetry::metrics::v1::metric::Data::ExponentialHistogram(
                                as_otel_exponential_histogram(eh, timestamp, duration, otel_dimensions.clone())
                            )),
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
        statistic_set_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "min",
            statistic_set.min.into(),
        ),
        statistic_set_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "max",
            statistic_set.max.into(),
        ),
        statistic_set_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "sum",
            statistic_set.sum.into(),
        ),
        statistic_set_component(
            full_measurement_name,
            timestamp_nanos,
            start_nanos,
            attributes,
            "count",
            statistic_set.count.into(),
        ),
    ]
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

fn statistic_set_component(
    full_measurement_name: &str,
    unix_nanos: u64,
    start_time_unix_nanos: u64,
    attributes: &[KeyValue],
    component: &str,
    value: opentelemetry::metrics::v1::number_data_point::Value,
) -> opentelemetry::metrics::v1::Metric {
    Metric {
        name: format!("{full_measurement_name}_{component}"),
        data: Some(opentelemetry::metrics::v1::metric::Data::Sum(Sum {
            aggregation_temporality: THE_ONLY_SANE_TEMPORALITY,
            is_monotonic: false,
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
    mut histogram: Histogram,
    timestamp: SystemTime,
    duration: Duration,
    attributes: Vec<KeyValue>,
) -> opentelemetry::metrics::v1::Histogram {
    let timestamp_nanos = timestamp.nanos_since_epoch();
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
        aggregation_temporality: THE_ONLY_SANE_TEMPORALITY,
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
    mut exponential_histogram: ExponentialHistogram,
    timestamp: SystemTime,
    duration: Duration,
    attributes: Vec<KeyValue>,
) -> opentelemetry::metrics::v1::ExponentialHistogram {
    let timestamp_nanos = timestamp.nanos_since_epoch();

    opentelemetry::metrics::v1::ExponentialHistogram {
        aggregation_temporality: THE_ONLY_SANE_TEMPORALITY,
        data_points: vec![ExponentialHistogramDataPoint {
            attributes,
            start_time_unix_nano: timestamp_nanos - duration.as_nanos() as u64,
            time_unix_nano: timestamp_nanos,
            count: exponential_histogram.count() as u64,
            sum: if exponential_histogram.has_negatives() {
                0_f64
            } else {
                exponential_histogram.sum() as f64
            },
            exemplars: vec![],                                // just no.
            flags: DataPointFlags::FlagNone as u32,           // i don't send useless buckets
            min: exponential_histogram.min(),                 // just use the histogram...
            max: exponential_histogram.max(),                 // just use the histogram...
            scale: exponential_histogram.scale() as i32,
            zero_count: 0, // I don't do this yet
            positive: Some(Buckets {
                // TODO: decide what to do about dynamic Scale scaling
                offset: 0,
                bucket_counts: exponential_histogram.take_positives().into_iter().map(|u| u as u64).collect()
            }),
            negative: Some(Buckets {
                // TODO: decide what to do about dynamic Scale scaling
                offset: 0,
                bucket_counts: exponential_histogram.take_negatives().into_iter().map(|u| u as u64).collect()
            }),
        }],
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use crate::{
        downstream::{
            channel_connection::get_channel,
            opentelemetry_downstream::{
                create_preaggregated_opentelemetry_batch, OpenTelemetryDownstream,
            },
        },
        metrics::Metrics,
        pipeline::{
            aggregator::{Aggregator, DistributionMode},
            stream_sink::StreamSink,
        },
    };

    #[test_log::test(tokio::test)]
    async fn downstream_is_runnable() {
        let (_sink, receiver) = StreamSink::<Box<Metrics>>::new();
        let aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
        let (batch_sender, batch_receiver) = mpsc::channel(128);

        let downstream = OpenTelemetryDownstream::new(
            get_channel("localhost:6379", || None, None).expect(
                "i can make a channel to localhost even though it probably isn't listening",
            ),
            None,
        );

        let metrics_tasks = tokio::task::LocalSet::new();

        let aggregator_handle = metrics_tasks.spawn_local(aggregator.aggregate_metrics_forever(
            Duration::from_millis(10),
            batch_sender,
            create_preaggregated_opentelemetry_batch,
        ));
        let downstream_joiner = metrics_tasks
            .spawn_local(async move { downstream.send_batches_forever(batch_receiver).await });

        aggregator_handle.abort();
        downstream_joiner.abort();
        metrics_tasks.await;
    }
}
