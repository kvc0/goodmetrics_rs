use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use tokio::{sync::mpsc, time::MissedTickBehavior};

use crate::{
    allocator::{
        returning_reference::{ReturnTarget, ReturningRef},
        MetricsAllocator, MetricsRef,
    },
    gauge::StatisticSetGauge,
    gauge_group::GaugeGroup,
    metrics::MetricsBehavior,
    pipeline::{
        aggregation::Aggregation,
        aggregator::{AggregatedMetricsMap, DimensionPosition, DimensionedMeasurementsMap},
        Sink,
    },
    types::Name,
};

/// Example complete preaggregated metrics pipeline setup, with gauge support:
///
/// ```
/// # let runtime = tokio::runtime::Builder::new_current_thread().build().expect("runtime can be built");
/// # runtime.block_on(async {
/// use goodmetrics::allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator;
/// use goodmetrics::downstream::goodmetrics_downstream::create_preaggregated_goodmetrics_batch;
/// use goodmetrics::downstream::goodmetrics_downstream::GoodmetricsDownstream;
/// use goodmetrics::metrics::Metrics;
/// use goodmetrics::metrics_factory::MetricsFactory;
/// use goodmetrics::pipeline::aggregator::Aggregator;
/// use goodmetrics::pipeline::aggregator::DistributionMode;
/// use goodmetrics::pipeline::stream_sink::StreamSink;
///
/// // 1. Make your metrics factory:
/// let (metrics_sink, raw_metrics_receiver) = StreamSink::new();
/// let aggregator = Aggregator::new(raw_metrics_receiver, DistributionMode::Histogram);
/// let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>> = MetricsFactory::new(metrics_sink);
/// let metrics_factory = std::sync::Arc::new(metrics_factory); // For sharing around!
///
/// // 2. Configure your delivery pipeline:
/// let downstream = GoodmetricsDownstream::new(
///     tonic::transport::Channel::from_static("https://[::1]:50051").connect_lazy(),
///     [("application", "example")],
/// );
///
/// // 3. Configure your background jobs:
/// let (aggregated_batch_sender, aggregated_batch_receiver) = tokio::sync::mpsc::channel(128);
/// // Configure Aggregator cadence and protocol.
/// tokio::task::spawn(
///     aggregator.aggregate_metrics_forever(
///         std::time::Duration::from_secs(1),
///         aggregated_batch_sender.clone(),
///         create_preaggregated_goodmetrics_batch,
///     )
/// );
/// // Send batches to the downstream collector, whatever you have.
/// tokio::task::spawn(
///     downstream.send_batches_forever(aggregated_batch_receiver)
/// );
/// // Register the gauge task for this metrics factory.
/// tokio::task::spawn(
///     metrics_factory.clone().report_gauges_forever(
///         std::time::Duration::from_secs(1),
///         aggregated_batch_sender,
///         create_preaggregated_goodmetrics_batch,
///     )
/// );
///
/// // Now you use your metrics_factory and clone the Arc around wherever you need it
/// # });
/// ```
pub struct MetricsFactory<TMetricsAllocator, TSink> {
    allocator: TMetricsAllocator,
    default_metrics_behavior: u32,
    sink: TSink,
    disabled: bool,
    gauge_groups: Mutex<HashMap<Name, GaugeGroup>>,
}

impl<TMetricsAllocator, TSink> Clone for MetricsFactory<TMetricsAllocator, TSink>
where
    TSink: Clone,
    TMetricsAllocator: Clone,
{
    /// Cloning a MetricsFactory is not free. It's not terrible but you should
    /// cache it rather than cloning repeatedly.
    fn clone(&self) -> Self {
        Self {
            allocator: self.allocator.clone(),
            default_metrics_behavior: self.default_metrics_behavior,
            sink: self.sink.clone(),
            disabled: self.disabled,
            gauge_groups: Default::default(),
        }
    }
}

pub trait RecordingScope<'a, TMetricsRef: 'a>: ReturnTarget<'a, TMetricsRef>
where
    Self: Sized,
{
    /// The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    fn record_scope(&'a self, scope_name: impl Into<Name>) -> ReturningRef<'a, TMetricsRef, Self>;

    /// The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    fn record_scope_with_behavior(
        &'a self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<'a, TMetricsRef, Self>;

    fn emit(&self, metrics: TMetricsRef);

    /// # Safety
    ///
    /// You should strongly consider using record_scope() instead.
    /// You _must_ emit() the returned instance through this MetricsFactory instance
    /// or else you may leak memory, depending on the semantics of your allocator.
    unsafe fn create_new_raw_metrics(&'a self, metrics_name: impl Into<Name>) -> TMetricsRef;
}

impl<'a, TMetricsRef, TMetricsAllocator, TSink> ReturnTarget<'a, TMetricsRef>
    for MetricsFactory<TMetricsAllocator, TSink>
where
    TMetricsRef: MetricsRef + 'a,
    TMetricsAllocator: MetricsAllocator<'a, TMetricsRef>,
    TSink: Sink<TMetricsRef>,
{
    fn return_referent(&self, to_return: TMetricsRef) {
        self.emit(to_return);
    }
}

impl<'a, TMetricsRef, TMetricsAllocator, TSink> RecordingScope<'a, TMetricsRef>
    for MetricsFactory<TMetricsAllocator, TSink>
where
    TMetricsRef: MetricsRef + 'a,
    TSink: Sink<TMetricsRef>,
    TMetricsAllocator: MetricsAllocator<'a, TMetricsRef>,
{
    #[inline]
    fn record_scope(&'a self, scope_name: impl Into<Name>) -> ReturningRef<'a, TMetricsRef, Self> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }

    #[inline]
    fn record_scope_with_behavior(
        &'a self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<'a, TMetricsRef, Self> {
        ReturningRef::new(self, unsafe {
            let mut m = self.create_new_raw_metrics(scope_name);
            m.as_mut().add_behavior(behavior);
            m
        })
    }

    // You should consider using record_scope() instead.
    #[inline]
    fn emit(&self, mut metrics: TMetricsRef) {
        if metrics.as_ref().has_behavior(MetricsBehavior::Suppress) {
            return;
        }
        if !metrics
            .as_ref()
            .has_behavior(MetricsBehavior::SuppressTotalTime)
        {
            let elapsed = metrics.as_ref().start_time.elapsed();
            metrics.as_mut().distribution("totaltime", elapsed);
        }

        self.sink.accept(metrics)
    }

    /// # Safety
    ///
    /// You should strongly consider using record_scope() instead.
    /// You _must_ emit() the returned instance through this MetricsFactory instance
    /// or else you may leak memory, depending on the semantics of your allocator.
    #[inline]
    unsafe fn create_new_raw_metrics(&'a self, metrics_name: impl Into<Name>) -> TMetricsRef {
        let mut m = self.allocator.new_metrics(metrics_name);
        m.as_mut().set_raw_behavior(self.default_metrics_behavior);
        if self.disabled {
            m.as_mut().add_behavior(MetricsBehavior::Suppress)
        }
        m
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink> {
    /// Get a gauge within a group, of a particular name.
    ///
    /// Gauges are aggregated as StatisticSet and passed to your downstream collector.
    ///
    /// You should cache the gauge that this function gives you. Gauges are threadsafe and fully non-blocking,
    /// but their registration and lifecycle are governed by Mutex.
    ///
    /// Gauges are less flexible than Metrics, but they can enable convenient high frequency recording.
    pub fn gauge(
        &self,
        gauge_group: impl Into<Name>,
        gauge_name: impl Into<Name>,
    ) -> Arc<StatisticSetGauge> {
        let mut locked_groups = self
            .gauge_groups
            .lock()
            .expect("local mutex should not be poisoned");
        let gauge_group = gauge_group.into();
        match locked_groups.get_mut(&gauge_group) {
            Some(group) => group.gauge(gauge_name),
            None => {
                let mut group = GaugeGroup::default();
                let gauge = group.gauge(gauge_name);
                locked_groups.insert(gauge_group, group);
                gauge
            }
        }
    }

    /// You'll want to schedule this in your runtime if you are using Gauges.
    pub async fn report_gauges_forever<TMakeBatchFunction, TBatch>(
        self: Arc<Self>,
        period: Duration,
        sender: mpsc::Sender<TBatch>,
        make_batch: TMakeBatchFunction,
    ) where
        TMakeBatchFunction:
            Fn(SystemTime, Duration, &mut AggregatedMetricsMap) -> TBatch + Send + 'static,
        TBatch: Send + 'static,
    {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let now = SystemTime::now();
            let mut gauges: AggregatedMetricsMap = self
                .gauge_groups
                .lock()
                .expect("local mutex should not be poisoned")
                .iter_mut()
                .map(|(group_name, gauge_group)| {
                    (
                        group_name.to_owned(),
                        DimensionedMeasurementsMap::from_iter(vec![(
                            DimensionPosition::new(),
                            gauge_group
                                .reset()
                                .map(|(name, statistic_set)| {
                                    (name, Aggregation::StatisticSet(statistic_set))
                                })
                                .collect(),
                        )]),
                    )
                })
                .collect();
            match sender.try_send(make_batch(now, period, &mut gauges)) {
                Ok(_) => log::debug!("reported batch"),
                Err(e) => {
                    log::error!("could not report gauges: {e:?}")
                }
            }
        }
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink> {
    pub fn disable(&mut self) {
        self.disabled = true
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink>
where
    TMetricsAllocator: Default,
{
    pub fn new(sink: TSink) -> Self {
        MetricsFactory::new_with_behaviors(sink, &[MetricsBehavior::Default])
    }

    pub fn new_with_behaviors(sink: TSink, behaviors: &[MetricsBehavior]) -> Self {
        MetricsFactory::new_with_allocator(sink, behaviors, Default::default())
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink> {
    pub fn new_with_allocator(
        sink: TSink,
        behaviors: &[MetricsBehavior],
        allocator: TMetricsAllocator,
    ) -> Self {
        MetricsFactory {
            allocator,
            default_metrics_behavior: behaviors
                .iter()
                .fold(0, |i, behavior| (i | (*behavior as u32))),
            sink,
            disabled: false,
            gauge_groups: Default::default(),
        }
    }
}

impl<TMetricsAllocator, TSink> Default for MetricsFactory<TMetricsAllocator, TSink>
where
    TSink: Default,
    TMetricsAllocator: Default,
{
    fn default() -> Self {
        Self::new(Default::default())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        allocator::{
            always_new_metrics_allocator::AlwaysNewMetricsAllocator,
            arc_allocator::{ArcAllocator, CachedMetrics},
        },
        metrics::{Metrics, MetricsBehavior},
        metrics_factory::RecordingScope,
        pipeline::{
            aggregator::{Aggregator, DistributionMode},
            logging_sink::LoggingSink,
            serializing_sink::SerializingSink,
            stream_sink::StreamSink,
        },
    };

    use super::MetricsFactory;

    #[test_log::test]
    fn logging_metrics_factory() {
        let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, LoggingSink> =
            MetricsFactory::new(LoggingSink::default());
        let mut metrics = metrics_factory.record_scope("test");
        // Dimension the scoped metrics
        metrics.dimension("some dimension", "a dim");

        // Measure some plain number
        metrics.measurement("measure", 13);

        // Record 1 observation of a distribution
        metrics.distribution("distribution of", 61);

        // Record many observations of a distribution
        metrics.distribution("high frequency", vec![13, 13, 14, 10, 13, 11, 13]);
    }

    #[test_log::test]
    fn serializing_metrics_factory() {
        let metrics_factory: MetricsFactory<
            AlwaysNewMetricsAllocator,
            SerializingSink<LoggingSink>,
        > = MetricsFactory::new_with_allocator(
            SerializingSink::new(LoggingSink::default()),
            &[MetricsBehavior::Default],
            AlwaysNewMetricsAllocator::default(),
        );
        let mut metrics = metrics_factory.record_scope("test");
        // Dimension the scoped metrics
        metrics.dimension("some dimension", "a dim");

        // metrics_factory.clone(); currently SerializingSink does not support cloning.
    }

    #[test_log::test]
    fn aggregating_metrics_factory() {
        let (stream_sink, receiver) = StreamSink::new();
        let _aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
        let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>> =
            MetricsFactory::new_with_allocator(
                stream_sink,
                &[MetricsBehavior::Default],
                AlwaysNewMetricsAllocator::default(),
            );
        #[allow(clippy::redundant_clone)]
        let cloned = metrics_factory.clone();
        {
            let mut metrics = metrics_factory.record_scope("test");
            metrics.dimension("some dimension", "a dim");
        }

        let _metrics_that_shares_the_sink = cloned.record_scope("scope_name");
    }

    #[test_log::test]
    fn aggregating_metrics_factory_with_arc_allocator() {
        let (stream_sink, receiver) = StreamSink::new();
        let _aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
        let metrics_factory: MetricsFactory<ArcAllocator<_>, StreamSink<CachedMetrics<_>>> =
            MetricsFactory::new_with_allocator(
                stream_sink,
                &[MetricsBehavior::Default],
                ArcAllocator::new(1024),
            );
        #[allow(clippy::redundant_clone)]
        let cloned = metrics_factory.clone();
        {
            let mut metrics = metrics_factory.record_scope("test");
            metrics.dimension("some dimension", "a dim");
        }

        let _metrics_that_shares_the_sink = cloned.record_scope("scope_name");
    }
}
