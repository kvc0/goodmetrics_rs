use std::{collections::HashMap, time::Instant};

use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};

use super::{Hasher, MetricsAllocator};

#[cfg(feature = "introspect")]
static INTROSPECTION_ALWAYS_NEW_INSTANCE_COUNT: crate::introspect::LazySumGauge =
    crate::introspect::LazySumGauge::new(Name::Str("always_new_instance"));

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
        #[cfg(feature = "introspect")]
        if let Some(gauge) = INTROSPECTION_ALWAYS_NEW_INSTANCE_COUNT.gauge() {
            gauge.observe(1)
        }

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
