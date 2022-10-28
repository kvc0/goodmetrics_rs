use std::{collections::HashMap, hash::BuildHasher, time::Instant};

use crate::{metrics::Metrics, types::Name};

use super::MetricsAllocator;

pub struct AlwaysNewMetricsAllocator {}
impl<TBuildHasher> MetricsAllocator<TBuildHasher> for AlwaysNewMetricsAllocator
where
    TBuildHasher: BuildHasher + Default,
{
    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> Metrics<TBuildHasher> {
        Metrics::new(
            metrics_name,
            Instant::now(),
            HashMap::with_hasher(Default::default()),
            HashMap::with_hasher(Default::default()),
        )
    }

    #[inline]
    fn drop_metrics(&self, _dropped: Metrics<TBuildHasher>) {
        // Allow the metrics to RAII away
    }
}
