use std::sync::Arc;

use crate::{
    allocator::{MetricsAllocator, MetricsRef, ReturnTarget, ReturningRef},
    metrics::MetricsBehavior,
    pipeline::Sink,
    types::Name,
};

/// Example complete preaggregated metrics pipeline setup, with gauge support:
///
/// ```
/// # let runtime = tokio::runtime::Builder::new_current_thread().build().expect("runtime can be built");
/// # runtime.block_on(async {
/// use goodmetrics::allocator::AlwaysNewMetricsAllocator;
/// use goodmetrics::downstream::get_client;
/// use goodmetrics::downstream::GoodmetricsBatcher;
/// use goodmetrics::downstream::GoodmetricsDownstream;
/// use goodmetrics::GaugeFactory;
/// use goodmetrics::Metrics;
/// use goodmetrics::MetricsFactory;
/// use goodmetrics::pipeline::Aggregator;
/// use goodmetrics::pipeline::DistributionMode;
/// use goodmetrics::pipeline::StreamSink;
///
/// // 1. Make your metrics factory:
/// let (metrics_sink, raw_metrics_receiver) = StreamSink::new();
/// let aggregator = Aggregator::new(raw_metrics_receiver, DistributionMode::Histogram);
/// let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>> = MetricsFactory::new(metrics_sink);
/// let metrics_factory = std::sync::Arc::new(metrics_factory); // For sharing around!
///
/// // 2. Configure your delivery pipeline:
/// let downstream = GoodmetricsDownstream::new(
///     get_client(
///         "https://ingest.example.com",
///         || None,
///         goodmetrics::proto::goodmetrics::metrics_client::MetricsClient::with_origin,
///     ).expect("i can make a channel to goodmetrics"),
///     Some(("authorization", "token".parse().expect("must be able to parse header"))),
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
///         GoodmetricsBatcher,
///     )
/// );
/// // Send batches to the downstream collector, whatever you have.
/// tokio::task::spawn(
///     downstream.send_batches_forever(aggregated_batch_receiver)
/// );
/// // Are you using gauges? Remember to spawn the background task for them too!
/// let gauge_factory = GaugeFactory::default();
/// tokio::task::spawn(
///     gauge_factory.clone().report_gauges_forever(
///         std::time::Duration::from_secs(1),
///         aggregated_batch_sender,
///         GoodmetricsBatcher,
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
        }
    }
}

impl<'a, TMetricsAllocator, TSink> ReturnTarget<TMetricsAllocator::TMetricsRef>
    for &'a MetricsFactory<TMetricsAllocator, TSink>
where
    TMetricsAllocator: MetricsAllocator + 'static,
    TMetricsAllocator::TMetricsRef: MetricsRef,
    TSink: Sink<TMetricsAllocator::TMetricsRef> + 'static,
{
    fn return_referent(&self, to_return: TMetricsAllocator::TMetricsRef) {
        self.emit(to_return);
    }
}

impl<TMetricsAllocator, TSink> ReturnTarget<TMetricsAllocator::TMetricsRef>
    for Arc<MetricsFactory<TMetricsAllocator, TSink>>
where
    TMetricsAllocator: MetricsAllocator + 'static,
    TMetricsAllocator::TMetricsRef: MetricsRef,
    TSink: Sink<TMetricsAllocator::TMetricsRef> + 'static,
{
    fn return_referent(&self, to_return: TMetricsAllocator::TMetricsRef) {
        self.emit(to_return);
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink>
where
    TSink: Sink<TMetricsAllocator::TMetricsRef> + 'static,
    TMetricsAllocator: MetricsAllocator + 'static,
    TMetricsAllocator::TMetricsRef: MetricsRef,
{
    /// The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    pub fn record_scope(
        &self,
        scope_name: impl Into<Name>,
    ) -> ReturningRef<TMetricsAllocator::TMetricsRef, &Self> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }

    /// The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    pub fn record_scope_with_behavior(
        &self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<TMetricsAllocator::TMetricsRef, &Self> {
        ReturningRef::new(self, unsafe {
            let mut m = self.create_new_raw_metrics(scope_name);
            m.as_mut().add_behavior(behavior);
            m
        })
    }

    /// The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    ///
    /// Use this when you need an owned Metrics object - for example, to write a metrics Interceptor for a tonic service.
    ///
    /// ```
    /// # use goodmetrics::{MetricsFactory, Metrics, allocator::{AlwaysNewMetricsAllocator, ReturningRef}, pipeline::StreamSink};
    /// # use std::sync::Arc;
    /// type MetricsFactoryType = Arc<MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>>>;
    ///
    /// fn get_metrics(shared_factory: MetricsFactoryType) -> ReturningRef<Metrics, MetricsFactoryType> {
    ///     shared_factory.record_scope_owned("returned from a locally owned factory")
    /// }
    /// ```
    pub fn record_scope_owned(
        self: Arc<Self>,
        scope_name: impl Into<Name>,
    ) -> ReturningRef<TMetricsAllocator::TMetricsRef, Arc<Self>> {
        let metrics = unsafe { self.create_new_raw_metrics(scope_name) };
        ReturningRef::new(self, metrics)
    }

    /// Called with the metrics ref that was vended via record_scope
    /// You should consider using record_scope() instead.
    pub(crate) fn emit(&self, mut metrics: TMetricsAllocator::TMetricsRef) {
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
    pub(crate) unsafe fn create_new_raw_metrics(
        &self,
        metrics_name: impl Into<Name>,
    ) -> TMetricsAllocator::TMetricsRef {
        let mut m = self.allocator.new_metrics(metrics_name);
        m.as_mut().set_raw_behavior(self.default_metrics_behavior);
        if self.disabled {
            m.as_mut().add_behavior(MetricsBehavior::Suppress)
        }
        m
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink> {
    /// Disable all metrics vended by this factory
    pub fn disable(&mut self) {
        self.disabled = true
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink>
where
    TMetricsAllocator: Default,
{
    /// Create a metrics factory that forwards complete metrics to sink
    pub fn new(sink: TSink) -> Self {
        MetricsFactory::new_with_behaviors(sink, &[MetricsBehavior::Default])
    }

    /// Create a metrics factory with custom behaviors that forwards complete metrics to sink
    pub fn new_with_behaviors(sink: TSink, behaviors: &[MetricsBehavior]) -> Self {
        MetricsFactory::new_with_allocator(sink, behaviors, Default::default())
    }
}

impl<TMetricsAllocator, TSink> MetricsFactory<TMetricsAllocator, TSink> {
    /// Create a metrics factory with custom behaviors and an explicit allocator that forwards complete metrics to sink
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
        allocator::{AlwaysNewMetricsAllocator, ArcAllocator, CachedMetrics},
        metrics::{Metrics, MetricsBehavior},
        pipeline::{Aggregator, DistributionMode, LoggingSink, SerializingSink, StreamSink},
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
            AlwaysNewMetricsAllocator,
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
                AlwaysNewMetricsAllocator,
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
