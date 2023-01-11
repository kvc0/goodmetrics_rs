use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::BuildHasher,
    marker::PhantomData,
    time::Instant,
};

use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

use super::MetricsAllocator;

/// Allocator which always creates a new instance.
/// Probably you will want PooledMetricsAllocator if you are doing
/// something with very tight timing constraints.
#[derive(Clone)]
pub struct AlwaysNewMetricsAllocator<TBuildHasher = RandomState> {
    _phantom: PhantomData<TBuildHasher>,
}

impl<T: BuildHasher> AlwaysNewMetricsAllocator<T> {
    pub fn new() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl Default for AlwaysNewMetricsAllocator<RandomState> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<'a, TBuildHasher> MetricsAllocator<'a, Box<Metrics<TBuildHasher>>>
    for AlwaysNewMetricsAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + 'a,
{
    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Box<Metrics<TBuildHasher>> {
        Box::new(Metrics::new(
            metrics_name,
            Instant::now(),
            HashMap::with_hasher(Default::default()),
            HashMap::with_hasher(Default::default()),
            MetricsBehavior::Default as u32,
        ))
    }
}
