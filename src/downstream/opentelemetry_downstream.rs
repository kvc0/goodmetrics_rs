use std::{
    cmp::Reverse,
    collections::{hash_map, BinaryHeap},
    sync::mpsc::Receiver,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_timer::Delay;

use crate::{
    pipeline::aggregating_sink::{
        bucket_10_below_2_sigfigs, Aggregation, DimensionPosition, DimensionedMeasurementsMap,
        Histogram, StatisticSet,
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

use super::{channel_connection::ChannelType, EpochTime};

// anything other than Delta is bugged by design. So yeah, opentelemetry metrics spec is bugged by design.
const THE_ONLY_SANE_TEMPORALITY: i32 = AggregationTemporality::Delta as i32;

pub struct OpenTelemetryDownstream {
    client: MetricsServiceClient<ChannelType>,
}

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

impl OpenTelemetryDownstream {
    pub fn new(channel: ChannelType) -> Self {
        let client: MetricsServiceClient<ChannelType> = MetricsServiceClient::new(channel);

        OpenTelemetryDownstream { client }
    }

    pub async fn send_batches_forever(&mut self, receiver: Receiver<Vec<Metric>>) {
        let interval = Duration::from_secs(1);
        loop {
            match receiver.try_recv() {
                Ok(batch) => {
                    let future = self.client.export(ExportMetricsServiceRequest {
                        resource_metrics: vec![ResourceMetrics {
                            resource: None,
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
                    log::info!("no metrics activity: {timeout}");
                    Delay::new(interval).await;
                }
            }
        }
    }
}

pub fn create_preaggregated_opentelemetry_batch(
    timestamp: SystemTime,
    duration: Duration,
    batch: hash_map::Drain<'_, Name, DimensionedMeasurementsMap>,
) -> Vec<Metric> {
    batch
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

#[cfg(test)]
mod test {
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };

    use crate::{
        downstream::{
            channel_connection::get_channel,
            opentelemetry_downstream::{
                create_preaggregated_opentelemetry_batch, OpenTelemetryDownstream,
            },
        },
        pipeline::aggregating_sink::AggregatingSink,
    };

    #[test_log::test(tokio::test)]
    async fn downstream_is_runnable() {
        let sink = Arc::new(AggregatingSink::new());
        let (sender, receiver) = mpsc::sync_channel(128);

        let mut downstream = OpenTelemetryDownstream::new(
            get_channel("localhost:6379", || None, None).await.expect(
                "i can make a channel to localhost even though it probably isn't listening",
            ),
        );

        let metrics_tasks = tokio::task::LocalSet::new();
        let task_sink = sink.clone();
        let sink_joiner = metrics_tasks.spawn_local(async move {
            task_sink
                .drain_into_sender_forever(
                    Duration::from_secs(1),
                    sender,
                    create_preaggregated_opentelemetry_batch,
                )
                .await
        });

        let downstream_joiner = metrics_tasks
            .spawn_local(async move { downstream.send_batches_forever(receiver).await });

        sink_joiner.abort();
        downstream_joiner.abort();
        metrics_tasks.await;
    }
}
