use std::{
    collections::HashMap,
    hash::BuildHasher,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    time::Instant,
};

use crate::{metrics::Metrics, types::Name};

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
        self.allocator.drop_metrics(metrics);
    }

    // You should consider using record_scope() instead.
    // You need to emit the returned instance or else you may leak memory, depending on the
    // semantics of your allocator.
    #[inline]
    pub unsafe fn create_new_raw_metrics(&self, metrics_name: impl Into<Name>) -> Metrics {
        self.allocator.new_metrics(metrics_name)
    }

    // The MetricsScope, when completed, records a `totaltime` in nanoseconds.
    #[inline]
    pub fn record_scope(
        &'a self,
        scope_name: impl Into<Name>,
    ) -> MetricsScope<'a, TMetricsAllocator> {
        MetricsScope::new(self, unsafe { self.create_new_raw_metrics(scope_name) })
    }
}

pub struct MetricsScope<'a, TMetricsAllocator: MetricsAllocator> {
    metrics_factory: &'a MetricsFactory<TMetricsAllocator>,
    metrics: ManuallyDrop<Metrics>,
}

impl<'a, TMetricsAllocator: MetricsAllocator> MetricsScope<'a, TMetricsAllocator> {
    #[inline]
    pub fn new(metrics_factory: &'a MetricsFactory<TMetricsAllocator>, metrics: Metrics) -> Self {
        Self {
            metrics_factory,
            metrics: ManuallyDrop::new(metrics),
        }
    }

    #[inline]
    unsafe fn take_ownership_of_metrics_memory(&mut self) -> Metrics {
        ManuallyDrop::take(&mut self.metrics)
    }
}

impl<'a, TMetricsAllocator: MetricsAllocator> Deref for MetricsScope<'a, TMetricsAllocator> {
    type Target = Metrics;

    #[inline]
    fn deref(&self) -> &Metrics {
        &self.metrics
    }
}

impl<'a, TMetricsAllocator: MetricsAllocator> DerefMut for MetricsScope<'a, TMetricsAllocator> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Metrics {
        &mut self.metrics
    }
}

impl<'a, TMetricsAllocator: MetricsAllocator> Drop for MetricsScope<'a, TMetricsAllocator> {
    #[inline]
    fn drop(&mut self) {
        // Transfer the metrics to the factory for finalization and emitting.
        // ManuallyDrop is used for fancy 0-cost ownership transfer.
        unsafe {
            self.metrics_factory
                .emit(self.take_ownership_of_metrics_memory())
        }
    }
}

// Extension point for integration with fine crates like https://docs.rs/object-pool/latest/object_pool/
pub trait MetricsAllocator<TBuildHasher = std::collections::hash_map::RandomState> {
    // Return a clean Metrics instance, possibly reused.
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Metrics<TBuildHasher>;

    // Metrics objects from new_metrics make their way back here eventually.
    fn drop_metrics(&self, dropped: Metrics);
}

pub struct AlwaysNewMetricsAllocator {}
impl<TBuildHasher> MetricsAllocator<TBuildHasher> for AlwaysNewMetricsAllocator
where
    TBuildHasher: BuildHasher + Default,
{
    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Metrics<TBuildHasher> {
        Metrics::new(
            metrics_name,
            Instant::now(),
            HashMap::with_hasher(Default::default()),
            HashMap::with_hasher(Default::default()),
        )
    }

    #[inline]
    fn drop_metrics(&self, _dropped: Metrics) {
        // Allow the metrics to RAII away
    }
}
