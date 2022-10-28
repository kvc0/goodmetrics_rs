use crate::{
    allocator::{
        always_new_metrics_allocator::AlwaysNewMetricsAllocator,
        returning_reference::{ReturnTarget, ReturningRef},
        MetricsAllocator,
    },
    metrics::Metrics,
    types::Name,
};

pub struct MetricsFactory<TMetricsAllocator = AlwaysNewMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator,
{
    allocator: TMetricsAllocator,
}

impl<'a, TMetricsAllocator> MetricsFactory<TMetricsAllocator>
where
    TMetricsAllocator: MetricsAllocator,
{
    // You should consider using record_scope() instead.
    #[inline]
    pub fn emit(&self, metrics: Metrics) {
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
        self.allocator.new_metrics(metrics_name)
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    pub fn record_scope(
        &'a self,
        scope_name: impl Into<Name>,
    ) -> ReturningRef<'a, Metrics, MetricsFactory<TMetricsAllocator>> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }
}

impl<TMetricsAllocator: MetricsAllocator> ReturnTarget<Metrics>
    for MetricsFactory<TMetricsAllocator>
{
    fn return_referent(&self, to_return: Metrics) {
        self.emit(to_return);
    }
}
