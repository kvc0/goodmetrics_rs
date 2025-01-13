use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, SystemTime},
};

use tokio::{sync::mpsc, time::MissedTickBehavior};

use crate::{
    pipeline::{AggregatedMetricsMap, AggregationBatcher},
    Gauge, GaugeDimensions, GaugeGroup, Name,
};

/// The default gauge factory. You should use this unless you have some fancy multi-factory setup.
///
/// Remember to report_gauges_forever() at the start of your program if you use this factory.
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
    /// You should cache the gauge that this function gives you. Gauges are threadsafe and fully non-blocking,
    /// but their registration and lifecycle are governed by Mutex.
    ///
    /// Gauges are less flexible than Metrics, but they can enable convenient high frequency recording.
    pub fn gauge_statistic_set(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
    ) -> Arc<Gauge> {
        self.dimensioned_gauge_statistic_set(gauge_group, gauge_name, Default::default())
    }

    /// Get a gauge within a group, of a particular name, with specified dimensions.
    pub fn dimensioned_gauge_statistic_set(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
    ) -> Arc<Gauge> {
        self.get_gauge(
            gauge_group,
            gauge_name,
            gauge_dimensions,
            crate::gauge::statistic_set_gauge,
        )
    }

    /// Get a gauge within a group, of a particular name, with specified dimensions.
    pub fn dimensioned_gauge_sum(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
    ) -> Arc<Gauge> {
        self.get_gauge(
            gauge_group,
            gauge_name,
            gauge_dimensions,
            crate::gauge::sum_gauge,
        )
    }

    fn get_gauge(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
        gauge_dimensions: GaugeDimensions,
        default: fn() -> Gauge,
    ) -> Arc<Gauge> {
        let mut locked_groups = self
            .gauge_groups
            .lock()
            .expect("local mutex should not be poisoned");
        let gauge_group = gauge_group.into();
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

        let result = receiver.recv().await.expect("should have received metrics");
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
            .get(&Name::from("test_dimensioned_gauges"))
            .expect("should have found data for gauge group `test_dimensioned_gauges`");

        assert_eq!(
            &HashMap::from([(
                BTreeMap::from([
                    (Name::from("test"), Dimension::from("dimension")),
                    (Name::from("other"), Dimension::from(1_u32)),
                ]),
                HashMap::from([
                    (
                        Name::from("dimensioned_gauge_one"),
                        Aggregation::StatisticSet(StatisticSet {
                            min: 100,
                            max: 100,
                            sum: 100,
                            count: 1,
                        })
                    ),
                    (
                        Name::from("dimensioned_gauge_two"),
                        Aggregation::StatisticSet(StatisticSet {
                            min: 10,
                            max: 10,
                            sum: 10,
                            count: 1,
                        })
                    ),
                ])
            ),]),
            dimensioned_gauges_result
        );
    }
}
