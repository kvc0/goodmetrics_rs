use std::{collections::HashMap, time::Instant};

use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

use super::{Hasher, MetricsAllocator};

/// Allocator which always creates a new instance.
/// You may want a pooled or arc allocator if you are doing
/// something with very tight timing constraints. You should
/// benchmark your application to be sure though. This is the
/// simplest way to use goodmetrics.
#[derive(Clone, Default)]
pub struct AlwaysNewMetricsAllocator;

impl MetricsAllocator for AlwaysNewMetricsAllocator {
    type TMetricsRef = Metrics<Hasher>;

    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Self::TMetricsRef {
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
