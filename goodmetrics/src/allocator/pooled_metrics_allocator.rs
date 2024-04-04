use std::{collections::HashMap, hash::BuildHasher, time::Instant};

use object_pool::{Pool, Reusable};

use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

use super::{Hasher, MetricsAllocator};

pub struct PooledMetricsAllocator<TBuildHasher = Hasher> {
    pool: Pool<Metrics<TBuildHasher>>,
    size: usize,
}

impl<TBuildHasher: BuildHasher + Default> Clone for PooledMetricsAllocator<TBuildHasher> {
    fn clone(&self) -> Self {
        Self::new(self.size)
    }
}

impl<T: BuildHasher + Default> PooledMetricsAllocator<T> {
    pub fn new(size: usize) -> Self {
        Self {
            pool: Pool::new(size, Self::instantiate_metrics),
            size,
        }
    }

    fn instantiate_metrics() -> Metrics<T> {
        Metrics::new(
            "",
            Instant::now(),
            HashMap::with_hasher(Default::default()),
            HashMap::with_hasher(Default::default()),
            Vec::new(),
            MetricsBehavior::Default as u32,
        )
    }
}

impl<H: BuildHasher + Default> Default for PooledMetricsAllocator<H> {
    fn default() -> Self {
        Self::new(128)
    }
}

/// Allocates a metrics instance from a pool - Metrics supports restart() so the
/// returned Metrics, while reused, are cleared of previous measurements.
impl<'a, TBuildHasher> MetricsAllocator<'a, Reusable<'a, Metrics<TBuildHasher>>>
    for PooledMetricsAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default,
{
    #[inline]
    fn new_metrics(&'a self, metrics_name: impl Into<Name>) -> Reusable<'a, Metrics<TBuildHasher>> {
        let mut metrics = self.pool.pull(Self::instantiate_metrics);
        metrics.restart();
        metrics.metrics_name = metrics_name.into();
        metrics
    }
}
