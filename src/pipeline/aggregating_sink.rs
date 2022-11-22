use std::{
    cmp::{max, min},
    collections::{hash_map, BTreeMap, HashMap},
    sync::{mpsc::SyncSender, Mutex},
    time::{Duration, Instant, SystemTime},
};

use futures_timer::Delay;

use crate::{
    allocator::MetricsRef,
    types::{self, Dimension, Measurement, Name},
};

use super::Sink;

// User-named metrics
pub type MetricsMap = HashMap<Name, DimensionedMeasurementsMap>;
// A metrics measurement family is grouped first by its dimension position
pub type DimensionedMeasurementsMap = HashMap<DimensionPosition, MeasurementAggregationMap>;
// A dimension position is a unique set of dimensions.
// If a measurement has (1) the same metric name, (2) the same dimensions and (3) the same measurement name as another measurement,
// it is the same measurement and they should be aggregated together.
pub type DimensionPosition = BTreeMap<Name, Dimension>;
// Within the dimension position there is a collection of named measurements; we'll store the aggregated view of these
pub type MeasurementAggregationMap = HashMap<Name, Aggregation>;

pub type Histogram = HashMap<i64, u64>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatisticSet {
    min: i64,
    max: i64,
    sum: i64,
    count: u64,
}
impl Default for StatisticSet {
    fn default() -> Self {
        Self {
            min: i64::MAX,
            max: i64::MIN,
            sum: 0,
            count: 0,
        }
    }
}
impl StatisticSet {
    fn accumulate<T: Into<i64>>(&mut self, value: T) {
        let v: i64 = value.into();
        self.min = min(v, self.min);
        self.max = max(v, self.max);
        self.sum += v;
        self.count += 1;
    }
}

trait HistogramAccumulate {
    fn accumulate<T: Into<i64>>(&mut self, value: T);
}
impl HistogramAccumulate for Histogram {
    fn accumulate<T: Into<i64>>(&mut self, value: T) {
        let v = value.into();
        let b = bucket_10_2_sigfigs(v);
        self.insert(b, self[&b] + 1);
    }
}

// For collecting and periodically reporting
#[derive(Debug, PartialEq, Eq)]
pub enum Aggregation {
    Histogram(Histogram),
    StatisticSet(StatisticSet),
}

pub struct AggregatingSink {
    map: Mutex<MetricsMap>,
}

impl Default for AggregatingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AggregatingSink {
    pub fn new() -> Self {
        AggregatingSink {
            map: Mutex::new(MetricsMap::default()),
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
        if map.len() < 1 {
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
                while None == (last_emit + cadence).checked_duration_since(now) {
                    last_emit += cadence;
                }
                Duration::ZERO
            };
            Delay::new(wait_for).await;

            if let Some(batch) = self.drain_into(SystemTime::now(), cadence, make_batch) {
                match sender.try_send(batch) {
                    Ok(_) => {
                        // Successfully sent
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
        let position: DimensionPosition = sunk_metrics.dimensions.drain().collect();
        let measurements_map: &mut MeasurementAggregationMap =
            dimensioned_measurements_map.entry(position).or_default();
        sunk_metrics
            .measurements
            .drain()
            .for_each(|(name, measurement)| match measurement {
                Measurement::Observation(observation) => {
                    accumulate_statisticset(measurements_map, name, observation);
                }
                Measurement::Distribution(distribution) => {
                    accumulate_distribution(measurements_map, name, distribution);
                }
            });
    }
}

fn accumulate_distribution(
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
        Aggregation::Histogram(histogram) => {
            match distribution {
                types::Distribution::I64(i) => {
                    histogram
                        .entry(bucket_10_2_sigfigs(i))
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                types::Distribution::I32(i) => {
                    histogram
                        .entry(bucket_10_2_sigfigs(i.into()))
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                types::Distribution::U64(i) => {
                    histogram
                        .entry(bucket_10_2_sigfigs(i as i64))
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                types::Distribution::U32(i) => {
                    histogram
                        .entry(bucket_10_2_sigfigs(i.into()))
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
                types::Distribution::Collection(collection) => {
                    collection.iter().for_each(|i| {
                        histogram
                            .entry(bucket_10_2_sigfigs(*i as i64))
                            .and_modify(|count| *count += 1)
                            .or_insert(1);
                    });
                }
            };
        }
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
    }
}

// impl<TMetricsRef> Sink<TMetricsRef> for AggregatingSink
// where
//     TMetricsRef: MetricsRef,
// {
//     fn accept(&self, metrics_ref: TMetricsRef) {
//         self.update_metrics_map(metrics_ref)
//     }
// }

// impl<TMetricsRef> Sink<TMetricsRef> for &AggregatingSink
// where
//     TMetricsRef: MetricsRef,
// {
//     fn accept(&self, metrics_ref: TMetricsRef) {
//         self.update_metrics_map(metrics_ref)
//     }
// }

impl<TSink, TMetricsRef> Sink<TMetricsRef> for TSink
where
    TSink: AsRef<AggregatingSink>,
    TMetricsRef: MetricsRef,
{
    fn accept(&self, metrics_ref: TMetricsRef) {
        self.as_ref().update_metrics_map(metrics_ref)
    }
}

// Base 10 significant-figures bucketing - toward 0
fn bucket_10<const FIGURES: u32>(value: i64) -> i64 {
    if value == 0 {
        return 0
    }
    // TODO: use i64.log10 when it's promoted to stable https://github.com/rust-lang/rust/issues/70887
    let power = ((value.abs() as f64).log10().ceil() as i32 - FIGURES as i32).max(0);
    let magnitude = 10_f64.powi(power);

    value.signum()
        // -> truncate off magnitude by dividing it away
        // -> ceil() away from 0 in both directions due to abs
        * (value.abs() as f64 / magnitude).ceil() as i64
        // restore original magnitude raised to the next figure if necessary
        * magnitude as i64
}

// Base 10 significant-figures bucketing - toward -inf
fn bucket_10_below<const FIGURES: u32>(value: i64) -> i64 {
    if value == 0 {
        return -1
    }
    // TODO: use i64.log10 when it's promoted to stable https://github.com/rust-lang/rust/issues/70887
    let power = ((value.abs() as f64).log10().ceil() as i32 - FIGURES as i32).max(0);
    let magnitude = 10_f64.powi(power);

    (value.signum()
        // -> truncate off magnitude by dividing it away
        // -> ceil() away from 0 in both directions due to abs
        * (value.abs() as f64 / magnitude).ceil() as i64 - 1)
        // restore original magnitude raised to the next figure if necessary
        * magnitude as i64
}

pub fn bucket_10_2_sigfigs(value: i64) -> i64 {
    bucket_10::<2>(value)
}

pub fn bucket_10_below_2_sigfigs(value: i64) -> i64 {
    bucket_10_below::<2>(value)
}

#[cfg(test)]
mod test {
    use std::{
        collections::{BTreeMap, HashMap},
        time::{Duration, Instant, SystemTime},
    };

    use crate::{
        allocator::{always_new_metrics_allocator::AlwaysNewMetricsAllocator, MetricsAllocator},
        metrics::Metrics,
        pipeline::aggregating_sink::{
            bucket_10_2_sigfigs, AggregatingSink, Aggregation, StatisticSet, bucket_10_below_2_sigfigs,
        },
        types::{Dimension, Name, Observation},
    };

    use super::DimensionedMeasurementsMap;

    #[test_log::test]
    fn test_bucket() {
        assert_eq!(0, bucket_10_2_sigfigs(0));
        assert_eq!(1, bucket_10_2_sigfigs(1));
        assert_eq!(-11, bucket_10_2_sigfigs(-11));

        assert_eq!(99, bucket_10_2_sigfigs(99));
        assert_eq!(100, bucket_10_2_sigfigs(100));
        assert_eq!(110, bucket_10_2_sigfigs(101));
        assert_eq!(110, bucket_10_2_sigfigs(109));
        assert_eq!(110, bucket_10_2_sigfigs(110));
        assert_eq!(120, bucket_10_2_sigfigs(111));

        assert_eq!(8000, bucket_10_2_sigfigs(8000));
        assert_eq!(8800, bucket_10_2_sigfigs(8799));
        assert_eq!(8800, bucket_10_2_sigfigs(8800));
        assert_eq!(8900, bucket_10_2_sigfigs(8801));

        assert_eq!(-8000, bucket_10_2_sigfigs(-8000));
        assert_eq!(-8800, bucket_10_2_sigfigs(-8799));
        assert_eq!(-8800, bucket_10_2_sigfigs(-8800));
        assert_eq!(-8900, bucket_10_2_sigfigs(-8801));
    }

    #[test_log::test]
    fn test_bucket_below() {
        assert_eq!(0, bucket_10_below_2_sigfigs(1));
        assert_eq!(-12, bucket_10_below_2_sigfigs(-11));

        assert_eq!(98, bucket_10_below_2_sigfigs(99));
        assert_eq!(99, bucket_10_below_2_sigfigs(100));
        assert_eq!(100, bucket_10_below_2_sigfigs(101));
        assert_eq!(100, bucket_10_below_2_sigfigs(109));
        assert_eq!(100, bucket_10_below_2_sigfigs(110));
        assert_eq!(110, bucket_10_below_2_sigfigs(111));

        assert_eq!(7900, bucket_10_below_2_sigfigs(8000));
        assert_eq!(8700, bucket_10_below_2_sigfigs(8799));
        assert_eq!(8700, bucket_10_below_2_sigfigs(8800));
        assert_eq!(8800, bucket_10_below_2_sigfigs(8801));

        assert_eq!(-8100, bucket_10_below_2_sigfigs(-8000));
        assert_eq!(-8900, bucket_10_below_2_sigfigs(-8799));
        assert_eq!(-8900, bucket_10_below_2_sigfigs(-8800));
        assert_eq!(-9000, bucket_10_below_2_sigfigs(-8801));
    }

    #[test_log::test]
    fn test_aggregation() {
        let sink: AggregatingSink = AggregatingSink::new();

        sink.update_metrics_map(get_metrics("a", "dimension", "v", 22));
        sink.update_metrics_map(get_metrics("a", "dimension", "v", 20));

        let map = sink.map.lock().unwrap();
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
        let sink: AggregatingSink = AggregatingSink::new();

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

        assert_eq!(HashMap::from([]), *sink.map.lock().unwrap());
    }

    fn get_metrics(
        dimension_name: impl Into<Name>,
        dimension: impl Into<Dimension>,
        measurement_name: impl Into<Name>,
        measurement: impl Into<Observation>,
    ) -> Box<Metrics> {
        let mut metrics = AlwaysNewMetricsAllocator::default().new_metrics("test");
        metrics.dimension(dimension_name, dimension);
        metrics.measurement(measurement_name, measurement);
        metrics
    }
}
