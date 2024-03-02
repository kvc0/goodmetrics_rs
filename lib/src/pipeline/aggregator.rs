use std::{
    cmp::min, collections::{BTreeMap, HashMap}, fmt::Display, mem::replace, time::{Duration, Instant, SystemTime}
};

use tokio::sync::mpsc;

use crate::{
    allocator::MetricsRef,
    types::{self, Dimension, Measurement, Name},
};

use super::{
    aggregation::{
        exponential_histogram::ExponentialHistogram, histogram::Histogram, online_tdigest::OnlineTdigest, statistic_set::StatisticSet, Aggregation
    },
    AbsorbDistribution,
};

/// User-named metrics
pub type AggregatedMetricsMap = HashMap<Name, DimensionedMeasurementsMap>;
/// A metrics measurement family is grouped first by its dimension position
pub type DimensionedMeasurementsMap = HashMap<DimensionPosition, MeasurementAggregationMap>;
/// A dimension position is a unique set of dimensions.
/// If a measurement has (1) the same metric name, (2) the same dimensions and (3) the same measurement name as another measurement,
/// it is the same measurement and they should be aggregated together.
pub type DimensionPosition = BTreeMap<Name, Dimension>;
/// Within the dimension position there is a collection of named measurements; we'll store the aggregated view of these
pub type MeasurementAggregationMap = HashMap<Name, Aggregation>;

#[derive(Debug, Clone, Copy)]
pub enum DistributionMode {
    /// Follows the opentelemetry standard for histogram buckets.
    ExponentialHistogram {
        max_buckets: u16,
    },
    /// Less space-efficient, less performant, but easy to understand.
    Histogram,
    /// Fancy sparse sketch distributions. Currently only compatible with
    /// Goodmetrics downstream, and timescaledb via timescaledb_toolkit.
    /// You should prefer t-digests when they are available to you :-)
    TDigest,
}

impl Display for DistributionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributionMode::ExponentialHistogram { max_buckets: _ } => f.write_str("exponential_histogram"),
            DistributionMode::Histogram => f.write_str("histogram"),
            DistributionMode::TDigest => f.write_str("t_digest"),
        }
    }
}

pub type SleepFunction = dyn Fn(Duration) + Send + Sync;

/// Primarily for testing and getting really deep into some stuff, here's
/// a way to customize how you group aggregates over time.
pub enum TimeSource {
    SystemTime,
    DynamicTime {
        now_wall_clock: Box<dyn Fn() -> SystemTime + Send + Sync>,
        now_timer: Box<dyn Fn() -> Instant + Send + Sync>,
        sleep: Box<SleepFunction>,
    },
}
impl std::fmt::Debug for TimeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SystemTime => write!(f, "SystemTime"),
            Self::DynamicTime { .. } => f.debug_tuple("DynamicTime").finish(),
        }
    }
}
impl Default for TimeSource {
    fn default() -> Self {
        Self::SystemTime
    }
}

pub struct Aggregator<TMetricsRef> {
    metrics_queue: std::sync::mpsc::Receiver<TMetricsRef>,
    map: AggregatedMetricsMap,
    distribution_mode: DistributionMode,
    time_source: TimeSource,
    cached_position: DimensionPosition,
    /// A workaround for the tokio::sync::mpsc::Sender charging way too much time
    /// on send for waking the receiver task across runtimes.
    poll_interval: Duration,
}

impl<TMetricsRef> Aggregator<TMetricsRef>
where
    TMetricsRef: MetricsRef + Send + 'static,
{
    pub fn new(
        metrics_queue: std::sync::mpsc::Receiver<TMetricsRef>,
        distribution_mode: DistributionMode,
    ) -> Self {
        Self {
            metrics_queue,
            map: Default::default(),
            distribution_mode,
            time_source: Default::default(),
            cached_position: Default::default(),
            poll_interval: Duration::from_millis(5),
        }
    }

    pub fn new_with_time_source(
        metrics_queue: std::sync::mpsc::Receiver<TMetricsRef>,
        distribution_mode: DistributionMode,
        time_source: TimeSource,
    ) -> Self {
        Self {
            metrics_queue,
            map: Default::default(),
            distribution_mode,
            time_source,
            cached_position: Default::default(),
            poll_interval: Duration::from_millis(5),
        }
    }

    /// This task runs a lot. You might want to have a separate 1-2 thread runtime for metrics tasks.
    /// Note that this depends on tokio and the `time` feature.
    pub async fn aggregate_metrics_forever<TMakeBatchFunction, TBatch>(
        mut self,
        cadence: Duration,
        sender: mpsc::Sender<TBatch>,
        make_batch: TMakeBatchFunction,
    ) where
        TMakeBatchFunction: Fn(SystemTime, Duration, &mut AggregatedMetricsMap) -> TBatch,
    {
        // Try to align to some even column since the epoch. It helps make metrics better-aligned when systems have well-aligned clocks.
        // It's usually more convenient in grafana this way.
        let extra_start_offset = self
            .now_wall_clock()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("could not get system time")
            .as_millis()
            % cadence.as_millis();
        tokio::time::sleep(Duration::from_millis(extra_start_offset as u64)).await;
        let mut last_emit = self.now_timer();

        loop {
            self.receive_until_next_batch(last_emit, cadence).await;

            last_emit = self.now_timer();
            if let Some(batch) = self.drain_into(self.now_wall_clock(), cadence, &make_batch) {
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

    async fn receive_until_next_batch(&mut self, last_emit: Instant, cadence: Duration) {
        let mut look_for_more = true;
        while look_for_more {
            let now = self.now_timer();
            look_for_more = match now
                .checked_duration_since(last_emit)
                .and_then(|latency| cadence.checked_sub(latency))
            {
                Some(wait_for) => self.receive_one(wait_for).await,
                None => false,
            }
        }
    }

    async fn receive_one(&mut self, mut wait_for: Duration) -> bool {
        loop {
            match self.metrics_queue.try_recv() {
                Ok(more) => {
                    self.aggregate_metrics(more);
                    break true;
                }
                Err(_) => {
                    let delay = min(wait_for, self.poll_interval);
                    if delay.is_zero() {
                        break false;
                    }
                    tokio::time::sleep(delay).await;
                    wait_for -= delay;
                }
            }
        }
    }

    // Ensure that you don't tarry long in the drain callback. The aggregator is held up while you are draining.
    // This is to keep overhead relatively low; I don't want to charge you map growth over and over at least for
    // your bread and butter metrics.
    // Just drain into your target type (for example, a metrics batch to send to a goodmetricsd server) within
    // the callback. Send the request outside of this scope.
    fn drain_into<DrainFunction, TReturn>(
        &mut self,
        timestamp: SystemTime,
        duration: Duration,
        drain_into: &DrainFunction,
    ) -> Option<TReturn>
    where
        DrainFunction: Fn(SystemTime, Duration, &mut AggregatedMetricsMap) -> TReturn,
    {
        if self.map.is_empty() {
            return None;
        }

        Some(drain_into(timestamp, duration, &mut self.map))
    }

    fn aggregate_metrics(&mut self, mut sunk_metrics: TMetricsRef) {
        let metrics_name = replace(
            &mut sunk_metrics.as_mut().metrics_name,
            Name::Str("_uninitialized_"),
        );
        let (dimensions, measurements) = sunk_metrics.as_mut().drain();

        let dimensioned_measurements_map: &mut DimensionedMeasurementsMap =
            match self.map.get_mut(&metrics_name) {
                Some(existing) => existing,
                None => {
                    self.map.insert(metrics_name.clone(), Default::default());
                    self.map
                        .get_mut(&metrics_name)
                        .expect("I just inserted this 1 line above")
                }
            };

        self.cached_position.extend(dimensions.drain()); // Use the cached memory
        let measurements_map: &mut MeasurementAggregationMap =
            match dimensioned_measurements_map.get_mut(&self.cached_position) {
                Some(map) => map,
                None => {
                    dimensioned_measurements_map
                        .insert(self.cached_position.clone(), Default::default());
                    dimensioned_measurements_map
                        .get_mut(&self.cached_position)
                        .expect("I just inserted this 1 line above")
                }
            };
        self.cached_position.clear(); // Return the cached memory

        measurements
            .drain()
            .for_each(|(name, measurement)| match measurement {
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
                    DistributionMode::ExponentialHistogram { max_buckets } => {
                        accumulate_exponential_histogram(measurements_map, name, distribution, max_buckets)
                    }
                },
            });
    }

    fn now_wall_clock(&self) -> SystemTime {
        match &self.time_source {
            TimeSource::SystemTime => SystemTime::now(),
            TimeSource::DynamicTime { now_wall_clock, .. } => now_wall_clock(),
        }
    }

    fn now_timer(&self) -> Instant {
        match &self.time_source {
            TimeSource::SystemTime => Instant::now(),
            TimeSource::DynamicTime { now_timer, .. } => now_timer(),
        }
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
        Aggregation::ExponentialHistogram(eh) => eh.absorb(distribution),
    }
}

fn accumulate_exponential_histogram(
    measurements_map: &mut HashMap<Name, Aggregation>,
    name: Name,
    distribution: types::Distribution,
    max_buckets: u16,
) {
    match measurements_map
        .entry(name)
        // TODO: decide what to do with dynamic Scale scaling
        .or_insert_with(|| Aggregation::ExponentialHistogram(ExponentialHistogram::new_with_max_buckets(2, max_buckets)))
    {
        Aggregation::StatisticSet(_s) => {
            log::error!("conflicting measurement and distribution name")
        }
        Aggregation::Histogram(histogram) => histogram.absorb(distribution),
        Aggregation::TDigest(td) => td.absorb(distribution),
        Aggregation::ExponentialHistogram(eh) => eh.absorb(distribution),
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
        Aggregation::ExponentialHistogram(eh) => eh.absorb(distribution),
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
        Aggregation::ExponentialHistogram(_eh) => {
            log::error!("conflicting measurement and distribution name")
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::mpsc::sync_channel,
        time::{Duration, SystemTime},
    };

    use crate::{
        allocator::{always_new_metrics_allocator::AlwaysNewMetricsAllocator, MetricsAllocator},
        metrics::Metrics,
        pipeline::aggregator::{Aggregation, Aggregator, DistributionMode, StatisticSet},
        types::{Dimension, Name, Observation},
    };

    use super::DimensionedMeasurementsMap;

    #[test_log::test(tokio::test())]
    async fn test_aggregation() {
        let (sender, receiver) = sync_channel(16);
        let mut sink: Aggregator<Metrics> = Aggregator::new(receiver, DistributionMode::Histogram);

        sender
            .try_send(get_metrics("a", "dimension", "v", 22))
            .unwrap();
        sender
            .try_send(get_metrics("a", "dimension", "v", 20))
            .unwrap();

        assert!(sink.receive_one(Duration::from_millis(1)).await);
        assert!(sink.receive_one(Duration::from_millis(1)).await);
        assert!(
            !sink.receive_one(Duration::from_millis(1)).await,
            "I only sent 2"
        );

        let map = sink.map;
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
            map,
        )
    }

    #[test_log::test(tokio::test)]
    async fn test_draining() {
        let (sender, receiver) = sync_channel(16);
        let mut sink: Aggregator<Metrics> = Aggregator::new(receiver, DistributionMode::Histogram);

        sender
            .try_send(get_metrics("a", "dimension", "v", 22))
            .unwrap();
        sender
            .try_send(get_metrics("a", "dimension", "v", 20))
            .unwrap();

        assert!(sink.receive_one(Duration::from_millis(1)).await);
        assert!(sink.receive_one(Duration::from_millis(1)).await);
        assert!(
            !sink.receive_one(Duration::from_millis(1)).await,
            "I only sent 2"
        );

        let transformed: Vec<(Name, DimensionedMeasurementsMap)> = sink
            .drain_into(
                SystemTime::now(),
                Duration::from_secs(1),
                &|_, _, aggregated| aggregated.drain().collect(),
            )
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

        assert_eq!(HashMap::from([]), sink.map);
    }

    fn get_metrics(
        dimension_name: impl Into<Name>,
        dimension: impl Into<Dimension>,
        measurement_name: impl Into<Name>,
        measurement: impl Into<Observation>,
    ) -> Metrics {
        let mut metrics = AlwaysNewMetricsAllocator.new_metrics("test");
        metrics.dimension(dimension_name, dimension);
        metrics.measurement(measurement_name, measurement);
        metrics
    }
}
