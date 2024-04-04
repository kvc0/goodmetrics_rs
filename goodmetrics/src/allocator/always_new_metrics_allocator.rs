use std::{collections::HashMap, hash::BuildHasher, time::Instant};

use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

use super::MetricsAllocator;

/// Allocator which always creates a new instance.
/// You may want a pooled or arc allocator if you are doing
/// something with very tight timing constraints. You should
/// benchmark your application to be sure though. This is the
/// simplest way to use goodmetrics.
#[derive(Clone, Default)]
pub struct AlwaysNewMetricsAllocator;

impl<'a, TBuildHasher> MetricsAllocator<'a, Metrics<TBuildHasher>> for AlwaysNewMetricsAllocator
where
    TBuildHasher: BuildHasher + Default + 'a,
{
    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Metrics<TBuildHasher> {
        Metrics::new(
            metrics_name,
            Instant::now(),
            HashMap::with_hasher(Default::default()),
            HashMap::with_hasher(Default::default()),
            Vec::new(),
            MetricsBehavior::Default as u32,
        )
    }
}
