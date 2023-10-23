use std::{
    collections::HashMap,
    sync::{mpsc::SyncSender, Arc, Mutex},
    time::{Duration, SystemTime},
};

use tokio::time::MissedTickBehavior;

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
    TMetricsAllocator: MetricsAllocator<'a, TMetricsRef> + Default,
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
    TMetricsAllocator: MetricsAllocator<'a, TMetricsRef> + Default,
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
    fn emit(&self, metrics: TMetricsRef) {
        if metrics.as_ref().has_behavior(MetricsBehavior::Suppress) {
            return;
        }
        if !metrics
            .as_ref()
            .has_behavior(MetricsBehavior::SuppressTotalTime)
        {
            let elapsed = metrics.as_ref().start_time.elapsed();
            metrics.as_ref().distribution("totaltime", elapsed);
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
        &self,
        period: Duration,
        sender: SyncSender<TBatch>,
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
        allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator,
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
        let metrics = metrics_factory.record_scope("test");
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
        let metrics = metrics_factory.record_scope("test");
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
            let metrics = metrics_factory.record_scope("test");
            metrics.dimension("some dimension", "a dim");
        }

        let _metrics_that_shares_the_sink = cloned.record_scope("scope_name");
    }
}
