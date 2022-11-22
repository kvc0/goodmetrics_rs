use std::{
    array,
    collections::{hash_map, BinaryHeap, BTreeMap},
    sync::mpsc::Receiver,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH}, cmp::Reverse,
};

use crate::{
    pipeline::aggregating_sink::{
        Aggregation, DimensionPosition, DimensionedMeasurementsMap, Histogram, bucket_10_below_2_sigfigs,
    },
    proto::opentelemetry::{
        self,
        collector::metrics::v1::{
            metrics_service_client::MetricsServiceClient, ExportMetricsServiceRequest,
        },
        common::v1::{any_value::Value, AnyValue, InstrumentationScope, KeyValue},
        metrics::v1::{
            AggregationTemporality, HistogramDataPoint, Metric, ResourceMetrics, ScopeMetrics, DataPointFlags,
        },
    },
    types::{Dimension, Name},
};

use super::channel_connection::ChannelType;

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
            match receiver.recv_timeout(interval) {
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
                    let result = future.await;
                    log::debug!("sent metrics: {result:?}");
                }
                Err(timeout) => {
                    log::info!("no metrics activity: {timeout}");
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
                    Aggregation::StatisticSet(_) => todo!(),
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

fn as_otel_histogram(mut histogram: Histogram, timestamp: SystemTime, duration: Duration, attributes: Vec<KeyValue>) -> opentelemetry::metrics::v1::Histogram {
    let timestamp_nanos = timestamp.duration_since(UNIX_EPOCH).expect("could not get system time").as_nanos() as u64;
    let bucket_values_count = histogram.values().sum();
    // These 3 data structures could be grown and cached for reuse
    // We want this in min heap order, sorted by bucket
    let mut histogram: BinaryHeap<Reverse<(i64, u64)>> = histogram.drain().map(|pair| Reverse(pair)).collect();
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
            },
            None => {
                sorted_bounds.push(below as f64);
                sorted_counts.push(0);
            },
        };
        sorted_bounds.push(bucket as f64);
        sorted_counts.push(count);
    }

    // utterly obnoxious histogram definition by otlp metrics here. There's this magical implicit +inf bucket
    // that you have to provide a count for... with goodmetrics histograms we already know what the maximum
    // value bucket is.
    sorted_counts.push(0);

    opentelemetry::metrics::v1::Histogram {
        // anything other than Delta is bugged by design. So yeah, opentelemetry metrics spec is bugged by design.
        aggregation_temporality: AggregationTemporality::Delta as i32,
        data_points: vec![
            HistogramDataPoint {
                attributes: attributes,
                start_time_unix_nano: timestamp_nanos - duration.as_nanos() as u64,
                time_unix_nano: timestamp_nanos,
                count: bucket_values_count,
                explicit_bounds: sorted_bounds,
                bucket_counts: sorted_counts,
                exemplars: vec![], // just no.
                flags: DataPointFlags::FlagNone as u32, // i don't send useless buckets
                min: None, // just use the histogram...
                max: None, // just use the histogram...
                sum: None, // This is a bad bad field
            }
        ],
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{mpsc, Arc},
        time::{Duration, Instant},
    };

    use hyper::{header::HeaderName, http::HeaderValue};

    use crate::{
        downstream::{
            channel_connection::get_channel,
            opentelemetry_downstream::{
                create_preaggregated_opentelemetry_batch, OpenTelemetryDownstream,
            },
        },
        pipeline::aggregating_sink::AggregatingSink, metrics_factory::{MetricsFactory, RecordingScope}, allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator,
    };

    #[test_log::test(tokio::test)]
    async fn downstream_is_runnable() {
        let sink = Arc::new(AggregatingSink::new());
        let (sender, receiver) = mpsc::sync_channel(128);

        let mut downstream =
            OpenTelemetryDownstream::new(get_channel("localhost:6379", true, None).await.expect(
                "i can make a channel to localhost even though it probably isn't listening",
            ));

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

//    #[ignore = "you can run this if you want but it's for sanity checking, not for continuous testing"]
    #[test_log::test(tokio::test)]
    async fn lightstep_demo() {
        let sink = Arc::new(AggregatingSink::new());
        let (sender, receiver) = mpsc::sync_channel(128);

        let mut downstream =
            OpenTelemetryDownstream::new(get_channel("https://ingest.lightstep.com", true, Some((HeaderName::from_static("lightstep-access-token"), HeaderValue::from_static("123")))).await.expect(
                "i can make a channel to lightstep",
            ));

        let metrics_tasks = tokio::task::LocalSet::new();
        let task_sink = sink.clone();
        metrics_tasks.spawn_local(async move {
            task_sink
                .drain_into_sender_forever(
                    Duration::from_secs(1),
                    sender,
                    create_preaggregated_opentelemetry_batch,
                )
                .await
        });

        metrics_tasks
            .spawn_local(async move { downstream.send_batches_forever(receiver).await });

        let sink_handle = sink.clone();
        metrics_tasks.run_until(async move {
            // let sink = sink_handle.as_ref();
            let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, Arc<AggregatingSink>> = MetricsFactory::new(sink_handle);
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(600) {
                let metrics = metrics_factory.record_scope("demo");
                // metrics
            }
        }).await;
    }
}
