use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, SystemTime},
};

use tokio::{sync::mpsc, time::MissedTickBehavior};

use crate::{
    gauge::{Gauge, HistogramHandle, StatisticSetHandle, SumHandle},
    pipeline::{AggregatedMetricsMap, AggregationBatcher},
    GaugeDimensions, GaugeGroup, Name,
};

/// The default gauge factory. You should use this unless you have some fancy multi-factory setup.
///
/// Remember to report_gauges_forever() at the start of your program if you use this factory.
/// ________
/// Configuration:
/// ```rust
/// # use goodmetrics::{GaugeFactory, default_gauge_factory};
/// # use goodmetrics::downstream::{get_client, OpenTelemetryDownstream, OpentelemetryBatcher};
/// # use goodmetrics::proto::opentelemetry::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
/// # use tokio_rustls::rustls::RootCertStore;
/// # use std::time::Duration;
/// // Call at the start of your program
/// fn set_up_default_gauge_factory() {
///     // 1. Configure your delivery destination:
///     let downstream = OpenTelemetryDownstream::new_with_dimensions(
///         get_client(
///             "https://ingest.example.com",
///             || Some(RootCertStore {
///                 roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
///             }),
///             MetricsServiceClient::with_origin,
///         ).expect("i can make a channel to ingest.example.com"),
///         Some(("authorization", "token".parse().expect("must be able to parse header"))),
///         vec![("application", "example"), ("version", env!("CARGO_PKG_VERSION"))],
///     );
///
///     // 2. Connect the downstream to the gauge factory:
///     let (aggregated_batch_sender, aggregated_batch_receiver) = tokio::sync::mpsc::channel(128);
///     tokio::task::spawn(downstream.send_batches_forever(aggregated_batch_receiver));
///     tokio::task::spawn(
///         default_gauge_factory()
///             .clone()
///             .report_gauges_forever(
///                 Duration::from_secs(10),
///                 aggregated_batch_sender,
///                 OpentelemetryBatcher,
///             )
///     );
/// }
/// ```
pub fn default_gauge_factory() -> &'static GaugeFactory {
    static DEFAULT_GAUGE_FACTORY: LazyLock<GaugeFactory> = LazyLock::new(GaugeFactory::default);

    &DEFAULT_GAUGE_FACTORY
}

/// A handle for creating gauges.
///
/// It is cheap to copy around, but you should cache your gauges.
#[derive(Clone, Debug, Default)]
pub struct GaugeFactory {
    gauge_groups: Arc<Mutex<HashMap<Name, GaugeGroup>>>,
}

impl GaugeFactory {
    /// Get a gauge within a group, of a particular name.
    ///
    /// Gauges are aggregated as StatisticSet and passed to your downstream collector.
    ///
    /// Cache the handle: Registration is guarded by a central mutex, and cloning the handle is cheap.
    ///
    /// It is an error to use a gauge with the same group and name but different handle type.
    pub fn gauge_statistic_set(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
    ) -> StatisticSetHandle {
        self.dimensioned_gauge_statistic_set(gauge_group, gauge_name, Default::default())
    }

    /// Get a gauge within a group, of a particular name, with specified dimensions.
    ///
    /// StatisticSets are backed by lightweight platform atomics. They are very fast.
    ///
    /// Cache the handle: Registration is guarded by a central mutex, and cloning the handle is cheap.
    ///
    /// It is an error to use a gauge with the same group and name but different handle type.
    pub fn dimensioned_gauge_statistic_set(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
    ) -> StatisticSetHandle {
        StatisticSetHandle {
            gauge: self.get_gauge(
                gauge_group,
                gauge_name,
                gauge_dimensions,
                crate::gauge::statistic_set_gauge,
            ),
        }
    }

    /// Get a gauge within a group, of a particular name, with specified dimensions.
    ///
    /// Sums are backed by lightweight platform atomics. They are very fast.
    ///
    /// Cache the handle: Registration is guarded by a central mutex, and cloning the handle is cheap.
    ///
    /// It is an error to use a gauge with the same group and name but different handle type.
    pub fn dimensioned_gauge_sum(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
    ) -> SumHandle {
        SumHandle {
            gauge: self.get_gauge(
                gauge_group,
                gauge_name,
                gauge_dimensions,
                crate::gauge::sum_gauge,
            ),
        }
    }

    /// Get a histogram gauge within a group, of a particular name, with specified dimensions.
    ///
    /// Histograms are controlled by Mutex. Math is done under a lock, so they are slower than StatisticSets. They are still quick, but measure the impact if you care.
    ///
    /// Cache the handle: Registration is guarded by a central mutex, and cloning the handle is cheap.
    ///
    /// It is an error to use a gauge with the same group and name but different handle type.
    ///
    /// ```
    /// # use goodmetrics::{GaugeFactory, default_gauge_factory};
    /// # use goodmetrics::GaugeDimensions;
    /// # use std::time::Duration;
    /// let histogram = default_gauge_factory().dimensioned_gauge_histogram(
    ///     "environment",
    ///     "sleep_latency",
    ///     GaugeDimensions::new([("operating_system", "example")])
    /// );
    ///
    /// // Record a histogram of how much time it actually takes to sleep 1 nanosecond on this machine.
    /// for _ in 0..10000 {
    ///    let _time_guard = histogram.time();
    ///     std::thread::sleep(Duration::from_nanos(1));
    /// }
    /// ```
    pub fn dimensioned_gauge_histogram(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
    ) -> HistogramHandle {
        HistogramHandle {
            gauge: self.get_gauge(
                gauge_group,
                gauge_name,
                gauge_dimensions,
                crate::gauge::histogram_gauge,
            ),
        }
    }

    fn get_gauge<T>(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
        default: fn() -> T,
    ) -> Arc<Gauge>
    where
        T: Into<Gauge>,
    {
        let gauge_group = gauge_group.into();
        let mut locked_groups = self
            .gauge_groups
            .lock()
            .expect("local mutex should not be poisoned");
        match locked_groups.get_mut(&gauge_group) {
            Some(group) => group.dimensioned_gauge(gauge_name, gauge_dimensions.into(), default),
            None => {
                let mut group = GaugeGroup::default();
                let gauge = group.dimensioned_gauge(gauge_name, gauge_dimensions.into(), default);
                locked_groups.insert(gauge_group, group);
                gauge
            }
        }
    }

    /// You'll want to schedule this in your runtime if you are using Gauges.
    ///
    /// You can use a clone of self for this function.
    pub async fn report_gauges_forever<TAggregationBatcher>(
        self,
        period: Duration,
        sender: mpsc::Sender<TAggregationBatcher::TBatch>,
        mut batcher: TAggregationBatcher,
    ) where
        TAggregationBatcher: AggregationBatcher,
        TAggregationBatcher::TBatch: Send,
    {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let now = SystemTime::now();
            let mut gauges: AggregatedMetricsMap = self.aggregate_and_reset();
            match sender.try_send(batcher.batch_aggregations(now, period, &mut gauges)) {
                Ok(_) => log::debug!("reported batch"),
                Err(e) => {
                    log::error!("could not report gauges: {e:?}")
                }
            }
        }
    }

    pub(crate) fn aggregate_and_reset(&self) -> AggregatedMetricsMap {
        self.gauge_groups
            .lock()
            .expect("local mutex should not be poisoned")
            .iter_mut()
            .filter_map(|(group_name, gauge_group)| {
                let possible_dimensioned_measurements = gauge_group.reset();
                if possible_dimensioned_measurements.is_empty() {
                    None
                } else {
                    Some((group_name.to_owned(), possible_dimensioned_measurements))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::{BTreeMap, HashMap, HashSet},
        time::{Duration, SystemTime},
    };

    use crate::{
        aggregation::{Aggregation, StatisticSet},
        pipeline::{AggregatedMetricsMap, AggregationBatcher, DimensionedMeasurementsMap},
        Dimension, GaugeDimensions, GaugeFactory, Name,
    };

    #[test_log::test(tokio::test)]
    async fn gauges() {
        struct BatchTaker;
        impl AggregationBatcher for BatchTaker {
            type TBatch = HashMap<Name, DimensionedMeasurementsMap>;

            fn batch_aggregations(
                &mut self,
                _now: SystemTime,
                _covered_time: Duration,
                aggregations: &mut AggregatedMetricsMap,
            ) -> Self::TBatch {
                std::mem::take(aggregations)
            }
        }

        let (sender, mut receiver) = tokio::sync::mpsc::channel(128);

        let gauge_factory = GaugeFactory::default();

        let _unused_gauge_group =
            gauge_factory.gauge_statistic_set("unused_gauge_group", "unused_gauge");
        let non_dimensioned_gauge =
            gauge_factory.gauge_statistic_set("test_gauges", "non_dimensioned_gauge");
        let mut dimensions = GaugeDimensions::new([("test", "dimension")]);
        dimensions.insert("other", 1_u32);
        let dimensioned_gauge_one = gauge_factory.dimensioned_gauge_statistic_set(
            "test_dimensioned_gauges",
            "dimensioned_gauge_one",
            dimensions.clone(),
        );
        let dimensioned_gauge_two = gauge_factory.dimensioned_gauge_statistic_set(
            "test_dimensioned_gauges",
            "dimensioned_gauge_two",
            dimensions.clone(),
        );
        let dimensioned_histogram = gauge_factory.dimensioned_gauge_histogram(
            "test_dimensioned_gauges",
            "a histogram",
            dimensions.clone(),
        );
        {
            let _time_guard = dimensioned_histogram.time();
            std::thread::sleep(Duration::from_micros(1)); // I didn't mock time, but I just want a nonzero duration.
        }
        let _unused_gauge_in_gauge_group = gauge_factory.dimensioned_gauge_statistic_set(
            "test_dimensioned_gauges",
            "unused_gauge",
            GaugeDimensions::new([("unused", "dimension")]),
        );
        let _unused_gauge_with_same_dimensions = gauge_factory.dimensioned_gauge_statistic_set(
            "test_dimensioned_gauges",
            "unused_gauge",
            dimensions,
        );

        non_dimensioned_gauge.observe(20);
        non_dimensioned_gauge.observe(22);
        dimensioned_gauge_one.observe(100);
        dimensioned_gauge_two.observe(10);

        tokio::task::spawn(gauge_factory.clone().report_gauges_forever(
            Duration::from_millis(1),
            sender,
            BatchTaker,
        ));

        let mut result = receiver.recv().await.expect("should have received metrics");
        assert_eq!(
            HashSet::from([
                &Name::from("test_gauges"),
                &Name::from("test_dimensioned_gauges")
            ]),
            result.keys().collect::<HashSet<&Name>>()
        );

        assert_eq!(
            &HashMap::from([(
                BTreeMap::from([]),
                HashMap::from([(
                    Name::from("non_dimensioned_gauge"),
                    Aggregation::StatisticSet(StatisticSet {
                        min: 20,
                        max: 22,
                        sum: 42,
                        count: 2,
                    })
                )])
            )]),
            result
                .get(&Name::from("test_gauges"))
                .expect("should have found data for gauge group `test_gauges`")
        );

        let dimensioned_gauges_result = result
            .remove(&Name::from("test_dimensioned_gauges"))
            .expect("should have found data for gauge group `test_dimensioned_gauges`");

        assert_eq!(
            dimensioned_gauges_result
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![BTreeMap::from([
                (Name::from("test"), Dimension::from("dimension")),
                (Name::from("other"), Dimension::from(1_u32)),
            ])],
            "there should only be 1 dimension position in this aggregated map"
        );

        let mut aggregation_map = dimensioned_gauges_result
            .into_values()
            .next()
            .expect("There was a dimension position");
        assert_eq!(
            aggregation_map.remove(&Name::from("dimensioned_gauge_one")),
            Some(Aggregation::StatisticSet(StatisticSet {
                min: 100,
                max: 100,
                sum: 100,
                count: 1,
            }))
        );
        assert_eq!(
            aggregation_map.remove(&Name::from("dimensioned_gauge_two")),
            Some(Aggregation::StatisticSet(StatisticSet {
                min: 10,
                max: 10,
                sum: 10,
                count: 1,
            }))
        );
        let histogram = aggregation_map
            .remove(&Name::from("a histogram"))
            .expect("a histogram should be in the map");
        match histogram {
            Aggregation::ExponentialHistogram(histogram) => {
                let value_counts = histogram
                    .value_counts()
                    .filter(|(_, count)| *count != 0)
                    .collect::<Vec<_>>();
                assert_eq!(
                    value_counts.len(),
                    1,
                    "there should be 1 value in the histogram"
                );
                let (bucket_value, count) = value_counts[0];
                assert_eq!(count, 1, "the count should be 1");
                assert!(
                    (1.0..=1000000000.0).contains(&bucket_value),
                    "the bucket value should sensical, but was {bucket_value}"
                );
            }
            _ => panic!("expected histogram"),
        }
        assert_eq!(
            aggregation_map.len(),
            0,
            "all gauges should have been removed from the map"
        );

        // Now wait for the next interval, where we should have no data. This means goodmetrics is reporting sparse data.
        let result = receiver.recv().await.expect("should have received metrics");
        assert_eq!(
            HashMap::from([]),
            result,
            "Nothing was reported in the last interval, so the gauges should be empty."
        );

        // Now report a timer and make sure the next interval has the data
        {
            dimensioned_histogram.time();
            std::thread::sleep(Duration::from_micros(1));
        }
        let mut result = receiver.recv().await.expect("should have received metrics");
        let dimensioned_gauges_result = result
            .remove(&Name::from("test_dimensioned_gauges"))
            .expect("should have found data for gauge group `test_dimensioned_gauges`");

        assert_eq!(
            dimensioned_gauges_result
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![BTreeMap::from([
                (Name::from("test"), Dimension::from("dimension")),
                (Name::from("other"), Dimension::from(1_u32)),
            ])],
            "there should only be 1 dimension position in this aggregated map"
        );

        let mut aggregation_map = dimensioned_gauges_result
            .into_values()
            .next()
            .expect("There was a dimension position");
        let histogram = aggregation_map
            .remove(&Name::from("a histogram"))
            .expect("a histogram should be in the map");
        match histogram {
            Aggregation::ExponentialHistogram(histogram) => {
                let value_counts = histogram
                    .value_counts()
                    .filter(|(_, count)| *count != 0)
                    .collect::<Vec<_>>();
                assert_eq!(
                    value_counts.len(),
                    1,
                    "there should be 1 value in the histogram"
                );
                let (bucket_value, count) = value_counts[0];
                assert_eq!(count, 1, "the count should be 1");
                assert!(
                    (1.0..=1000000000.0).contains(&bucket_value),
                    "the bucket value should sensical, but was {bucket_value}"
                );
            }
            _ => panic!("expected histogram"),
        }
        assert_eq!(
            aggregation_map.len(),
            0,
            "all gauges should have been removed from the map"
        );
    }
}
