use crate::{
    allocator::{
        returning_reference::{ReturnTarget, ReturningRef},
        MetricsAllocator, MetricsRef,
    },
    metrics::MetricsBehavior,
    pipeline::Sink,
    types::Name,
};

pub struct MetricsFactory<TMetricsAllocator, TSink> {
    allocator: TMetricsAllocator,
    default_metrics_behavior: u32,
    sink: TSink,
}

/// Cloning a MetricsFactory is not cheap. You should cache it per-thread rather
/// than doing so to work around visibility issues.
impl<TMetricsAllocator, TSink> Clone for MetricsFactory<TMetricsAllocator, TSink>
where
    TSink: Clone,
    TMetricsAllocator: Clone,
{
    fn clone(&self) -> Self {
        Self {
            allocator: self.allocator.clone(),
            default_metrics_behavior: self.default_metrics_behavior,
            sink: self.sink.clone(),
        }
    }
}

pub trait RecordingScope<'a, TMetricsRef: 'a>: ReturnTarget<'a, TMetricsRef>
where
    Self: Sized,
{
    fn record_scope(&'a self, scope_name: impl Into<Name>) -> ReturningRef<'a, TMetricsRef, Self>;

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
    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    fn record_scope(&'a self, scope_name: impl Into<Name>) -> ReturningRef<'a, TMetricsRef, Self> {
        ReturningRef::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    fn record_scope_with_behavior(
        &'a self,
        scope_name: impl Into<Name>,
        behavior: MetricsBehavior,
    ) -> ReturningRef<'a, TMetricsRef, Self> {
        ReturningRef::new(self, unsafe {
            let mut m = self.create_new_raw_metrics(scope_name);
            m.add_behavior(behavior);
            m
        })
    }

    // You should consider using record_scope() instead.
    #[inline]
    fn emit(&self, metrics: TMetricsRef) {
        if metrics.has_behavior(MetricsBehavior::Suppress) {
            return;
        }
        if !metrics.has_behavior(MetricsBehavior::SuppressTotalTime) {
            let elapsed = metrics.start_time.elapsed();
            metrics.distribution("totaltime", elapsed);
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
        m.set_raw_behavior(self.default_metrics_behavior);
        m
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
    use std::rc::Rc;

    use crate::{
        allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator,
        metrics::MetricsBehavior,
        metrics_factory::RecordingScope,
        pipeline::{
            aggregating_sink::AggregatingSink, logging_sink::LoggingSink,
            serializing_sink::SerializingSink,
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
        let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, Rc<AggregatingSink>> =
            MetricsFactory::new_with_allocator(
                Rc::new(AggregatingSink::new()),
                &[MetricsBehavior::Default],
                AlwaysNewMetricsAllocator::default(),
            );
        {
            let metrics = metrics_factory.record_scope("test");
            metrics.dimension("some dimension", "a dim");
        }

        let cloned = metrics_factory.clone();
        let _metrics_that_shares_the_sink = cloned.record_scope("scope_name");
    }
}
