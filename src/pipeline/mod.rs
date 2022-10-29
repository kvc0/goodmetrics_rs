use std::hash::BuildHasher;

use crate::{
    allocator::{returning_reference::ReturningRef, MetricsAllocator},
    metrics::Metrics,
};

pub mod logging_sink;

pub trait Sink<
    TMetricsAllocator: MetricsAllocator<TBuildHasher>,
    TBuildHasher: BuildHasher = std::collections::hash_map::RandomState,
>
{
    fn accept(&self, metrics_ref: ReturningRef<Metrics<TBuildHasher>, TMetricsAllocator>);
}
