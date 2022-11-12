use std::{
    cmp::{max, min},
    collections::{BTreeMap, HashMap},
    sync::mpsc::{self, Receiver, SyncSender},
};

use crate::{
    allocator::MetricsRef,
    types::{Dimension, Measurement, Name},
};

use super::Sink;

// User-named metrics
type MetricsMap = HashMap<Name, DimensionedMeasurementsMap>;
// A metrics measurement family is grouped first by its dimension position
type DimensionedMeasurementsMap = HashMap<DimensionPosition, MeasurementAggregationMap>;
// A dimension position is a unique set of dimensions.
// If a measurement has (1) the same metric name, (2) the same dimensions and (3) the same measurement name as another measurement,
// it is the same measurement and they should be aggregated together.
type DimensionPosition = BTreeMap<Name, Dimension>;
// Within the dimension position there is a collection of named measurements; we'll store the aggregated view of these
type MeasurementAggregationMap = HashMap<Name, Aggregation>;

type Histogram = HashMap<i64, u64>;

struct StatisticSet {
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

enum Aggregation {
    Histogram(Histogram),
    StatisticSet(StatisticSet),
}

pub struct AggregatingSink<TMetricsRef> {
    map: MetricsMap,
    sender: SyncSender<TMetricsRef>,
    receiver: Receiver<TMetricsRef>,
}

impl<TMetricsRef> Default for AggregatingSink<TMetricsRef>
where
    TMetricsRef: MetricsRef,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<TMetricsRef> AggregatingSink<TMetricsRef>
where
    TMetricsRef: MetricsRef,
{
    pub fn new_with_bound(bound: usize) -> Self {
        let (sender, receiver) = mpsc::sync_channel(bound);
        AggregatingSink {
            map: MetricsMap::default(),
            sender,
            receiver,
        }
    }

    pub fn new() -> Self {
        Self::new_with_bound(1024)
    }

    pub fn run_aggregator_forever(&mut self) {
        while let Ok(mut sunk) = self.receiver.recv() {
            let dimensioned_measurements_map: &mut DimensionedMeasurementsMap =
                self.map.entry(sunk.metrics_name.clone()).or_default();

            let position: DimensionPosition = sunk.dimensions.drain().collect();
            let measurements_map: &mut MeasurementAggregationMap =
                dimensioned_measurements_map.entry(position).or_default();

            sunk.measurements
                .drain()
                .for_each(|(name, measurement)| match measurement {
                    Measurement::Observation(observation) => {
                        match measurements_map
                            .entry(name)
                            .or_insert_with(|| Aggregation::StatisticSet(StatisticSet::default()))
                        {
                            Aggregation::StatisticSet(statistic_set) => {
                                statistic_set.accumulate(observation)
                            }
                            Aggregation::Histogram(_h) => {
                                log::error!("conflicting measurement and distribution name")
                            }
                        }
                    }
                    Measurement::Distribution(distribution) => {
                        match measurements_map
                            .entry(name)
                            .or_insert_with(|| Aggregation::Histogram(Histogram::default()))
                        {
                            Aggregation::StatisticSet(_s) => {
                                log::error!("conflicting measurement and distribution name")
                            }
                            Aggregation::Histogram(histogram) => {
                                match distribution {
                                    crate::types::Distribution::I64(i) => {
                                        histogram
                                            .entry(bucket_10_2_sigfigs(i))
                                            .and_modify(|count| *count += 1)
                                            .or_insert(1);
                                    }
                                    crate::types::Distribution::I32(i) => {
                                        histogram
                                            .entry(bucket_10_2_sigfigs(i.into()))
                                            .and_modify(|count| *count += 1)
                                            .or_insert(1);
                                    }
                                    crate::types::Distribution::U64(i) => {
                                        histogram
                                            .entry(bucket_10_2_sigfigs(i as i64))
                                            .and_modify(|count| *count += 1)
                                            .or_insert(1);
                                    }
                                    crate::types::Distribution::U32(i) => {
                                        histogram
                                            .entry(bucket_10_2_sigfigs(i.into()))
                                            .and_modify(|count| *count += 1)
                                            .or_insert(1);
                                    }
                                    crate::types::Distribution::Collection(collection) => {
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
                });
        }
    }
}

impl<TMetricsRef> Sink<TMetricsRef> for AggregatingSink<TMetricsRef> {
    fn accept(&self, metrics_ref: TMetricsRef) {
        match self.sender.try_send(metrics_ref) {
            Ok(_) => {}
            Err(error) => {
                log::error!("could not send metrics to channel: {error}")
            }
        }
    }
}

// Base 10 significant-figures bucketing
fn bucket_10<const FIGURES: u32>(value: i64) -> i64 {
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

fn bucket_10_2_sigfigs(value: i64) -> i64 {
    bucket_10::<2>(value)
}

#[cfg(test)]
mod test {
    use crate::pipeline::aggregating_sink::bucket_10_2_sigfigs;

    #[test_log::test]
    fn test_bucket() {
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
}
