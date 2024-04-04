//! How Metrics objects are created and possibly cached.

#[cfg(not(feature = "ahash-hasher"))]
use std::collections::hash_map::RandomState;

#[cfg(feature = "ahash-hasher")]
use ahash::RandomState;

/// Alias for the default hasher, selected by the ahash-hasher crate feature
pub(crate) type Hasher = RandomState;

use crate::{metrics::Metrics, types::Name};

mod always_new_metrics_allocator;
mod arc_allocator;
mod pooled_metrics_allocator;
mod returning_reference;

pub use always_new_metrics_allocator::AlwaysNewMetricsAllocator;
pub use arc_allocator::{ArcAllocator, CachedMetrics};
pub use pooled_metrics_allocator::PooledMetricsAllocator;
pub use returning_reference::{ReturnTarget, ReturningRef};

/// Convenience trait for a way to refer to a Metrics or a reference generically
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
    /// Return a clean Metrics instance, possibly reused.
    fn new_metrics(&'a self, metrics_name: impl Into<Name>) -> TMetricsRef;
}
