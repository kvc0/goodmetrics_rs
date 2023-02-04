use std::{
    collections::hash_map::RandomState,
    ops::{Deref, DerefMut},
};

use crate::{metrics::Metrics, types::Name};

pub mod always_new_metrics_allocator;
pub mod returning_reference;

pub mod pooled_metrics_allocator;

/// A convenience for typedefs of smart references for Metrics objects. This is
/// used by the PooledMetricsAllocator to do object pooling while still allowing
/// you to opt out.
pub trait MetricsRef<TBuildHasher = RandomState>:
    Deref<Target = Metrics<TBuildHasher>> + DerefMut<Target = Metrics<TBuildHasher>>
{
}

impl<T, TBuildHasher> MetricsRef<TBuildHasher> for T where
    T: Deref<Target = Metrics<TBuildHasher>> + DerefMut<Target = Metrics<TBuildHasher>>
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
