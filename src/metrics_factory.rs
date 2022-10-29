use crate::{
    allocator::{
        always_new_metrics_allocator::AlwaysNewMetricsAllocator,
        returning_reference::{ReturnTarget, ReturningRef},
        MetricsAllocator,
    },
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

pub struct MetricsFactory<TMetricsAllocator = AlwaysNewMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator,
{
    allocator: TMetricsAllocator,
    default_metrics_behavior: u32,
}

impl<TMetricsAllocator> MetricsFactory<TMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator + Default,
{
    pub fn new() -> MetricsFactory<TMetricsAllocator> {
        MetricsFactory::new_with_behaviors(&[
            MetricsBehavior::Default,
            MetricsBehavior::RecordTotalTime,
        ])
    }

    pub fn new_with_behaviors(behaviors: &[MetricsBehavior]) -> MetricsFactory<TMetricsAllocator> {
        MetricsFactory::new_with_allocator(Default::default(), behaviors)
    }
}

impl<TMetricsAllocator> Default for MetricsFactory<TMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator + Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, TMetricsAllocator> MetricsFactory<TMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator,
{
    pub fn new_with_allocator(
        allocator: TMetricsAllocator,
        behaviors: &[MetricsBehavior],
    ) -> MetricsFactory<TMetricsAllocator> {
        MetricsFactory {
            allocator,
            default_metrics_behavior: behaviors.iter().fold(0, |i, behavior| i | *behavior as u32),
        }
    }

    // You should consider using record_scope() instead.
    #[inline]
    pub fn emit(&self, metrics: Metrics) {
        if metrics.has_behavior(MetricsBehavior::Suppress) {
            self.allocator.return_referent(metrics);
            return;
        }
        if metrics.has_behavior(MetricsBehavior::RecordTotalTime) {}
        log::debug!("emit metrics: {:?}", metrics);

        ReturningRef::new(&self.allocator, metrics);
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
    ) -> ReturningRef<'a, Metrics, MetricsFactory<TMetricsAllocator>> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    pub fn record_scope_with_behavior(
        &'a self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<'a, Metrics, MetricsFactory<TMetricsAllocator>> {
        ReturningRef::new(self, unsafe {
            let mut m = self.create_new_raw_metrics(scope_name);
            m.add_behavior(behavior);
            m
        })
    }
}

impl<TMetricsAllocator: MetricsAllocator> ReturnTarget<Metrics>
    for MetricsFactory<TMetricsAllocator>
{
    fn return_referent(&self, to_return: Metrics) {
        self.emit(to_return);
    }
}
