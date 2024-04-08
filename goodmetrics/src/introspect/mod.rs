//! The `introspect` feature.
//! This records and reports metrics about your metrics pipeline.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use arc_swap::ArcSwapOption;
use tokio::{select, sync::mpsc};

use crate::{
    allocator::AlwaysNewMetricsAllocator,
    pipeline::{AggregatedMetricsMap, Aggregator, DistributionMode, StreamSink},
    Metrics, MetricsFactory,
};

static INTROSPECTION_METRICS_FACTORY: ArcSwapOption<
    MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>>,
> = ArcSwapOption::const_empty();

/// Configuration for introspection metrics
pub struct IntrospectionConfiguration<TBatch, TMakeBatchFunction> {
    distribution_mode: DistributionMode,
    cadence: Duration,
    sender: mpsc::Sender<TBatch>,
    make_batch: TMakeBatchFunction,
}

impl<TBatch, TMakeBatchFunction> IntrospectionConfiguration<TBatch, TMakeBatchFunction> {
    /// Create a configuration for introspection metrics
    pub fn new(sender: mpsc::Sender<TBatch>, make_batch: TMakeBatchFunction) -> Self {
        Self {
            distribution_mode: DistributionMode::ExponentialHistogram {
                max_buckets: 160,
                desired_scale: 8,
            },
            cadence: Duration::from_secs(15),
            sender,
            make_batch,
        }
    }

    /// Set the reporting cadence (default 15s)
    pub fn cadence(&mut self, cadence: Duration) {
        self.cadence = cadence
    }

    /// Set the distribution mode (default ExponentialHistogram 160 buckets, desired scale 8)
    pub fn distribution_mode(&mut self, distribution_mode: DistributionMode) {
        self.distribution_mode = distribution_mode
    }
}

/// Spawn on a tokio runtime. This registers pipeline introspection
/// metrics to be emitted through the configured downstream sink.
pub async fn run_introspection_metrics<TBatch, TMakeBatchFunction>(
    configuration: IntrospectionConfiguration<TBatch, TMakeBatchFunction>,
) where
    TMakeBatchFunction:
        Fn(SystemTime, Duration, &mut AggregatedMetricsMap) -> TBatch + Send + 'static + Copy,
    TBatch: Send + 'static,
{
    let (sink, receiver) = StreamSink::new();
    let aggregator = Aggregator::new(receiver, configuration.distribution_mode);

    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>> =
        MetricsFactory::new(sink);
    let metrics_factory = Arc::new(metrics_factory);

    let gauge_future = metrics_factory.clone().report_gauges_forever(
        configuration.cadence,
        configuration.sender.clone(),
        configuration.make_batch,
    );
    let aggregator_future = aggregator.aggregate_metrics_forever(
        configuration.cadence,
        configuration.sender,
        configuration.make_batch,
    );

    INTROSPECTION_METRICS_FACTORY.store(Some(metrics_factory));

    select! {
        _ = gauge_future => {
            log::warn!("gauge collection ended - terminating introspection metrics")
        }
        _ = aggregator_future => {
            log::warn!("aggregator collection ended - terminating introspection metrics")
        }
    }
}

#[allow(unused)]
pub(crate) fn introspection_factory(
) -> Option<Arc<MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<Metrics>>>> {
    INTROSPECTION_METRICS_FACTORY.load().clone()
}

#[cfg(test)]
mod test {
    use super::introspection_factory;

    #[test]
    fn introspection() {
        assert!(introspection_factory().is_none());
    }
}
