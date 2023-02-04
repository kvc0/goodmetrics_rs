use std::{
    collections::{hash_map, BTreeMap, HashMap},
    sync::{mpsc::SyncSender, Mutex},
    time::{Duration, Instant, SystemTime},
};

use futures_timer::Delay;

use crate::{
    allocator::MetricsRef,
    types::{self, Dimension, Measurement, Name},
};

use super::{
    aggregation::{
        histogram::Histogram, online_tdigest::OnlineTdigest, statistic_set::StatisticSet,
        Aggregation,
    },
    AbsorbDistribution, Sink,
};

/// User-named metrics
pub type MetricsMap = HashMap<Name, DimensionedMeasurementsMap>;
/// A metrics measurement family is grouped first by its dimension position
pub type DimensionedMeasurementsMap = HashMap<DimensionPosition, MeasurementAggregationMap>;
/// A dimension position is a unique set of dimensions.
/// If a measurement has (1) the same metric name, (2) the same dimensions and (3) the same measurement name as another measurement,
/// it is the same measurement and they should be aggregated together.
pub type DimensionPosition = BTreeMap<Name, Dimension>;
/// Within the dimension position there is a collection of named measurements; we'll store the aggregated view of these
pub type MeasurementAggregationMap = HashMap<Name, Aggregation>;

#[derive(Debug)]
pub enum DistributionMode {
    /// Less space-efficient, less performant, but easy to understand.
    /// If you're using opentelemetry downstream, this is your only choice.
    Histogram,
    /// Fancy sparse sketch distributions. Currently only compatible with
    /// Goodmetrics downstream, and timescaledb via timescaledb_toolkit.
    /// You should prefer t-digests when they are available to you :-)
    TDigest,
}

/// A metrics sink that takes Metrics in, aggregates them, and emits them on some schedule.
pub struct AggregatingSink {
    map: Mutex<MetricsMap>,
    cached_position: Mutex<DimensionPosition>,
    distribution_mode: DistributionMode,
}

impl Default for AggregatingSink {
    /// Uses TDigest for distributions by default.
    fn default() -> Self {
        Self::new(DistributionMode::TDigest)
    }
}

impl AggregatingSink {
    pub fn new(distribution_mode: DistributionMode) -> Self {
        AggregatingSink {
            map: Default::default(),
            cached_position: Default::default(),
            distribution_mode,
        }
    }

    // Ensure that you don't tarry long in the drain callback. The aggregator is held up while you are draining.
    // This is to keep overhead relatively low; I don't want to charge you map growth over and over at least for
    // your bread and butter metrics.
    // Just drain into your target type (for example, a metrics batch to send to a goodmetricsd server) within
    // the callback. Send the request outside of this scope.
    pub fn drain_into<DrainFunction, TReturn>(
        &self,
        timestamp: SystemTime,
        duration: Duration,
        drain_into: DrainFunction,
    ) -> Option<TReturn>
    where
        DrainFunction: FnOnce(
            SystemTime,
            Duration,
            hash_map::Drain<'_, Name, DimensionedMeasurementsMap>,
        ) -> TReturn,
    {
        let mut map = self.map.lock().expect("must be able to access metrics map");
        if map.is_empty() {
            return None;
        }
        let metrics_drain = map.drain();

        Some(drain_into(timestamp, duration, metrics_drain))
    }

    pub async fn drain_into_sender_forever<TMakeBatchFunction, TBatch>(
        &self,
        cadence: Duration,
        sender: SyncSender<TBatch>,
        make_batch: TMakeBatchFunction,
    ) where
        TMakeBatchFunction: FnMut(
                SystemTime,
                Duration,
                hash_map::Drain<'_, Name, DimensionedMeasurementsMap>,
            ) -> TBatch
            + Copy,
    {
        // Try to align to some even column since the epoch. It helps make metrics better-aligned when systems have well-aligned clocks.
        // It's usually more convenient in grafana this way.
        let extra_start_offset = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("could not get system time")
            .as_millis()
            % cadence.as_millis();
        Delay::new(Duration::from_millis(extra_start_offset as u64)).await;
        let mut last_emit = Instant::now();

        loop {
            let now = Instant::now();
            let wait_for = if let Some(wait_for) = (last_emit + cadence).checked_duration_since(now)
            {
                last_emit += cadence;
                wait_for
            } else {
                while (last_emit + cadence).checked_duration_since(now).is_none() {
                    last_emit += cadence;
                }
                Duration::ZERO
            };
            Delay::new(wait_for).await;

            if let Some(batch) = self.drain_into(SystemTime::now(), cadence, make_batch) {
                match sender.try_send(batch) {
                    Ok(_) => {
                        log::info!("sent batch to sink")
                    }
                    Err(error) => {
                        log::error!("Failed to send metrics batch: {error}")
                    }
                }
            }
        }
    }

    fn update_metrics_map(&self, mut sunk_metrics: impl MetricsRef) {
        let mut map = self.map.lock().expect("must be able to access metrics map");
        let dimensioned_measurements_map: &mut DimensionedMeasurementsMap =
            map.entry(sunk_metrics.metrics_name.clone()).or_default();
        let (dimensions_drain, measurements_drain) = sunk_metrics.drain();

        let mut cached_position = self
            .cached_position
            .lock()
            .expect("must be able to access state");
        cached_position.extend(dimensions_drain);
        let measurements_map: &mut MeasurementAggregationMap =
            match dimensioned_measurements_map.get_mut(&cached_position) {
                Some(map) => map,
                None => dimensioned_measurements_map
                    .entry(cached_position.clone())
                    .or_default(),
            };
        cached_position.clear();
        drop(cached_position);
        measurements_drain.for_each(|(name, measurement)| match measurement {
            Measurement::Observation(observation) => {
                accumulate_statisticset(measurements_map, name, observation);
            }
            Measurement::Distribution(distribution) => match self.distribution_mode {
                DistributionMode::Histogram => {
                    accumulate_histogram(measurements_map, name, distribution);
                }
                DistributionMode::TDigest => {
                    accumulate_tdigest(measurements_map, name, distribution);
                }
            },
        });
    }
}

fn accumulate_histogram(
    measurements_map: &mut HashMap<Name, Aggregation>,
    name: Name,
    distribution: types::Distribution,
) {
    match measurements_map
        .entry(name)
        .or_insert_with(|| Aggregation::Histogram(Histogram::default()))
    {
        Aggregation::StatisticSet(_s) => {
            log::error!("conflicting measurement and distribution name")
        }
        Aggregation::Histogram(histogram) => histogram.absorb(distribution),
        Aggregation::TDigest(td) => td.absorb(distribution),
    }
}

fn accumulate_tdigest(
    measurements_map: &mut HashMap<Name, Aggregation>,
    name: Name,
    distribution: types::Distribution,
) {
    match measurements_map
        .entry(name)
        .or_insert_with(|| Aggregation::TDigest(OnlineTdigest::default()))
    {
        Aggregation::StatisticSet(_s) => {
            log::error!("conflicting measurement and distribution name")
        }
        Aggregation::Histogram(histogram) => histogram.absorb(distribution),
        Aggregation::TDigest(td) => td.absorb(distribution),
    }
}

fn accumulate_statisticset(
    measurements_map: &mut HashMap<Name, Aggregation>,
    name: Name,
    observation: types::Observation,
) {
    match measurements_map
        .entry(name)
        .or_insert_with(|| Aggregation::StatisticSet(StatisticSet::default()))
    {
        Aggregation::StatisticSet(statistic_set) => statistic_set.accumulate(observation),
        Aggregation::Histogram(_h) => {
            log::error!("conflicting measurement and distribution name")
        }
        Aggregation::TDigest(_td) => {
            log::error!("conflicting measurement and distribution name")
        }
    }
}

impl<TSink, TMetricsRef> Sink<TMetricsRef> for TSink
where
    TSink: AsRef<AggregatingSink>,
    TMetricsRef: MetricsRef,
{
    fn accept(&self, metrics_ref: TMetricsRef) {
        self.as_ref().update_metrics_map(metrics_ref)
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::{BTreeMap, HashMap},
        time::{Duration, SystemTime},
    };

    use crate::{
        allocator::{always_new_metrics_allocator::AlwaysNewMetricsAllocator, MetricsAllocator},
        metrics::Metrics,
        pipeline::aggregating_sink::{
            AggregatingSink, Aggregation, DistributionMode, StatisticSet,
        },
        types::{Dimension, Name, Observation},
    };

    use super::DimensionedMeasurementsMap;

    #[test_log::test]
    fn test_aggregation() {
        let sink: AggregatingSink = AggregatingSink::new(DistributionMode::Histogram);

        sink.update_metrics_map(get_metrics("a", "dimension", "v", 22));
        sink.update_metrics_map(get_metrics("a", "dimension", "v", 20));

        let map = sink
            .map
            .lock()
            .expect("should be able to lock the aggregation sink");
        assert_eq!(
            HashMap::from([(
                Name::from("test"),
                HashMap::from([(
                    BTreeMap::from([(Name::from("a"), Dimension::from("dimension"))]),
                    HashMap::from([(
                        Name::from("v"),
                        Aggregation::StatisticSet(StatisticSet {
                            min: 20,
                            max: 22,
                            sum: 42,
                            count: 2
                        })
                    )])
                )])
            )]),
            *map,
        )
    }

    #[test_log::test]
    fn test_draining() {
        let sink: AggregatingSink = AggregatingSink::new(DistributionMode::Histogram);

        sink.update_metrics_map(get_metrics("a", "dimension", "v", 22));
        sink.update_metrics_map(get_metrics("a", "dimension", "v", 20));

        let transformed: Vec<(Name, DimensionedMeasurementsMap)> = sink
            .drain_into(SystemTime::now(), Duration::from_secs(1), |_, _, drain| {
                drain.collect()
            })
            .expect("there should be contents in the batch");
        assert_eq!(
            Vec::from([(
                Name::from("test"),
                HashMap::from([(
                    BTreeMap::from([(Name::from("a"), Dimension::from("dimension"))]),
                    HashMap::from([(
                        Name::from("v"),
                        Aggregation::StatisticSet(StatisticSet {
                            min: 20,
                            max: 22,
                            sum: 42,
                            count: 2
                        })
                    )])
                )])
            )]),
            transformed,
        );

        assert_eq!(
            HashMap::from([]),
            *sink.map.lock().expect("should be able to lock the sink")
        );
    }

    fn get_metrics(
        dimension_name: impl Into<Name>,
        dimension: impl Into<Dimension>,
        measurement_name: impl Into<Name>,
        measurement: impl Into<Observation>,
    ) -> Box<Metrics> {
        let metrics = AlwaysNewMetricsAllocator::default().new_metrics("test");
        metrics.dimension(dimension_name, dimension);
        metrics.measurement(measurement_name, measurement);
        metrics
    }
}
