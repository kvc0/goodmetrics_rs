use std::hash::BuildHasher;

use crate::allocator::MetricsAllocator;

use super::Sink;

struct LoggingSink {
    log_level: log::Level,
}

impl<TMetricsAllocator, TBuildHasher> Sink<TMetricsAllocator, TBuildHasher> for LoggingSink
where
    TMetricsAllocator: MetricsAllocator<TBuildHasher>,
    TBuildHasher: BuildHasher + core::fmt::Debug,
{
    fn accept(
        &self,
        metrics_ref: crate::allocator::returning_reference::ReturningRef<
            crate::metrics::Metrics<TBuildHasher>,
            TMetricsAllocator,
        >,
    ) {
        log::log!(
            self.log_level,
            "recorded metrics for {}: {}",
            metrics_ref.name(),
            *metrics_ref
        )
    }
}
