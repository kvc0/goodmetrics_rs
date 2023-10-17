use std::collections::hash_map::RandomState;

use crate::{metrics::Metrics, types::Name};

pub mod always_new_metrics_allocator;
pub mod returning_reference;

pub mod pooled_metrics_allocator;

pub trait MetricsRef<TBuildHasher = RandomState>:
    AsRef<Metrics<TBuildHasher>> + AsMut<Metrics<TBuildHasher>>
{
}

impl<T, TBuildHasher> MetricsRef<TBuildHasher> for T where
    T: AsRef<Metrics<TBuildHasher>> + AsMut<Metrics<TBuildHasher>>
{
}

/// Extension for integration with fine crates like https://docs.rs/object-pool/latest/object_pool/
pub trait MetricsAllocator<'a, TMetricsRef>
where
    TMetricsRef: 'a,
{
    // Return a clean Metrics instance, possibly reused.
    fn new_metrics(&'a self, metrics_name: impl Into<Name>) -> TMetricsRef;
}
