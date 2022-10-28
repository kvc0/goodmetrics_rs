use std::hash::BuildHasher;

use crate::{metrics::Metrics, types::Name};

use self::returning_reference::ReturnTarget;

pub mod always_new_metrics_allocator;
pub mod returning_reference;

// Extension point for integration with fine crates like https://docs.rs/object-pool/latest/object_pool/
pub trait MetricsAllocator<TBuildHasher: BuildHasher = std::collections::hash_map::RandomState>:
    ReturnTarget<Metrics>
{
    // Return a clean Metrics instance, possibly reused.
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Metrics<TBuildHasher>;

    // Metrics objects from new_metrics make their way back here eventually.
    fn drop_metrics(&self, dropped: Metrics<TBuildHasher>);
}

// Blanket bridge implementation for any MetricsAllocator to behave as a ReturnTarget
impl<TBuildHasher: BuildHasher, U: MetricsAllocator<TBuildHasher>>
    ReturnTarget<Metrics<TBuildHasher>> for U
{
    fn return_referent(&self, to_return: Metrics<TBuildHasher>) {
        self.drop_metrics(to_return)
    }
}
