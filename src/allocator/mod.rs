use std::{
    collections::hash_map::RandomState,
    ops::{Deref, DerefMut},
};

use crate::{metrics::Metrics, types::Name};

pub mod always_new_metrics_allocator;
pub mod returning_reference;

pub trait MetricsRef<TBuildHasher = RandomState>:
    Deref<Target = Metrics<TBuildHasher>> + DerefMut<Target = Metrics<TBuildHasher>>
{
}

impl<T, TBuildHasher> MetricsRef<TBuildHasher> for T where
    T: Deref<Target = Metrics<TBuildHasher>> + DerefMut<Target = Metrics<TBuildHasher>>
{
}

// Extension point for integration with fine crates like https://docs.rs/object-pool/latest/object_pool/
pub trait MetricsAllocator<TMetricsRef> {
    // Return a clean Metrics instance, possibly reused.
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> TMetricsRef;
}
