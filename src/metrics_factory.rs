use crate::{
    allocator::{
        always_new_metrics_allocator::AlwaysNewMetricsAllocator,
        returning_reference::{ReturnTarget, ReturningRef},
        MetricsAllocator,
    },
    metrics::{Metrics, MetricsBehavior},
    types::Name, pipeline::Sink,
};

pub struct MetricsFactory<TSink, TMetricsAllocator = AlwaysNewMetricsAllocator>
where
    TSink: for <'a> Sink<ReturningRef<'a, Metrics, TMetricsAllocator>>,
    TMetricsAllocator: MetricsAllocator,
{
    allocator: TMetricsAllocator,
    default_metrics_behavior: u32,
    sink: TSink
}

impl<TSink, TMetricsAllocator> MetricsFactory<TSink, TMetricsAllocator>
where
    TSink: for<'a> Sink<ReturningRef<'a, Metrics, TMetricsAllocator>>,
    TMetricsAllocator: MetricsAllocator + Default,
{
    pub fn new(sink: TSink) -> MetricsFactory<TSink, TMetricsAllocator> {
        MetricsFactory::new_with_behaviors(
            sink,
            &[
                MetricsBehavior::Default,
            ]
        )
    }

    pub fn new_with_behaviors(sink: TSink, behaviors: &[MetricsBehavior]) -> MetricsFactory<TSink, TMetricsAllocator> {
        MetricsFactory::new_with_allocator(sink, behaviors, Default::default())
    }
}

impl<TSink, TMetricsAllocator> Default for MetricsFactory<TSink, TMetricsAllocator>
where
    TSink: for<'a> Sink<ReturningRef<'a, Metrics, TMetricsAllocator>> + Default,
    TMetricsAllocator: MetricsAllocator + Default,
{
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<'a, TSink, TMetricsAllocator> MetricsFactory<TSink, TMetricsAllocator>
where
    TSink: for<'b> Sink<ReturningRef<'b, Metrics, TMetricsAllocator>>,
    TMetricsAllocator: MetricsAllocator,
{
    pub fn new_with_allocator(
        sink: TSink,
        behaviors: &[MetricsBehavior],
        allocator: TMetricsAllocator,
    ) -> MetricsFactory<TSink, TMetricsAllocator> {
        MetricsFactory {
            allocator,
            default_metrics_behavior: behaviors.iter().fold(0, |i, behavior| i | *behavior as u32),
            sink,
        }
    }

    // You should consider using record_scope() instead.
    #[inline]
    pub fn emit(&self, mut metrics: Metrics) {
        if metrics.has_behavior(MetricsBehavior::Suppress) {
            self.allocator.return_referent(metrics);
            return;
        }
        if !metrics.has_behavior(MetricsBehavior::SuppressTotalTime) {
            metrics.distribution("totaltime", metrics.start_time.elapsed());
        }
        log::debug!("emit metrics: {:?}", metrics);

        let pipeline_ref = ReturningRef::new(&self.allocator, metrics);
        self.sink.accept(pipeline_ref)
    }

    /// # Safety
    ///
    /// You should strongly consider using record_scope() instead.
    /// You _must_ emit() the returned instance through this MetricsFactory instance
    /// or else you may leak memory, depending on the semantics of your allocator.
    #[inline]
    pub unsafe fn create_new_raw_metrics(&self, metrics_name: impl Into<Name>) -> Metrics {
        let mut m = self.allocator.new_metrics(metrics_name);
        m.set_raw_behavior(self.default_metrics_behavior);
        m
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    pub fn record_scope(
        &'a self,
        scope_name: impl Into<Name>,
    ) -> ReturningRef<'a, Metrics, MetricsFactory<TSink, TMetricsAllocator>> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    pub fn record_scope_with_behavior(
        &'a self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<'a, Metrics, MetricsFactory<TSink, TMetricsAllocator>> {
        ReturningRef::new(self, unsafe {
            let mut m = self.create_new_raw_metrics(scope_name);
            m.add_behavior(behavior);
            m
        })
    }
}

impl<TSink, TMetricsAllocator> ReturnTarget<Metrics>
    for MetricsFactory<TSink, TMetricsAllocator>
    where
        TSink: for<'a> Sink<ReturningRef<'a, Metrics, TMetricsAllocator>>,
        TMetricsAllocator: MetricsAllocator,
{
    fn return_referent(&self, to_return: Metrics) {
        self.emit(to_return);
    }
}

#[cfg(test)]
mod test {
    use crate::{allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator, pipeline::{serializing_sink::SerializingSink, logging_sink::LoggingSink}, metrics::{Metrics, MetricsBehavior}};

    use super::MetricsFactory;

    #[test_log::test]
    fn logging_metrics_factory() {
        let metrics_factory: MetricsFactory<LoggingSink, AlwaysNewMetricsAllocator> = MetricsFactory::new(LoggingSink::default());
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
        let metrics_factory = MetricsFactory::new_with_allocator(
            SerializingSink::new(LoggingSink::default()),
            &vec![MetricsBehavior::Default],
            AlwaysNewMetricsAllocator::default(),
        );
        let mut metrics = metrics_factory.record_scope("test");
        // Dimension the scoped metrics
        metrics.dimension("some dimension", "a dim");
    }
}
