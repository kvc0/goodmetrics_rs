use std::hash::BuildHasher;

use crate::{
    allocator::{returning_reference::ReturningRef, MetricsAllocator},
    metrics::Metrics,
};

pub trait Sink<
    TMetricsAllocator: MetricsAllocator<TBuildHasher>,
    TBuildHasher: BuildHasher = std::collections::hash_map::RandomState,
>
{
    fn accept(metrics_ref: ReturningRef<Metrics<TBuildHasher>, TMetricsAllocator>);
}

pub trait Pipeline {}
