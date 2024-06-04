use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwapOption;
use tokio::sync::mpsc;

use crate::{
    pipeline::{AggregationBatcher, DistributionMode},
    GaugeFactory,
};

static INTROSPECTION_GAUGE_FACTORY: ArcSwapOption<GaugeFactory> = ArcSwapOption::const_empty();

/// Configuration for introspection metrics
pub struct IntrospectionConfiguration<TAggregationBatcher: AggregationBatcher> {
    distribution_mode: DistributionMode,
    cadence: Duration,
    sender: mpsc::Sender<TAggregationBatcher::TBatch>,
    batcher: TAggregationBatcher,
}

impl<TAggregationBatcher: AggregationBatcher> IntrospectionConfiguration<TAggregationBatcher> {
    /// Create a configuration for introspection metrics
    pub fn new(
        sender: mpsc::Sender<TAggregationBatcher::TBatch>,
        batcher: TAggregationBatcher,
    ) -> Self {
        Self {
            distribution_mode: DistributionMode::ExponentialHistogram {
                max_buckets: 160,
                desired_scale: 8,
            },
            cadence: Duration::from_secs(15),
            sender,
            batcher,
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
pub async fn run_introspection_metrics<TAggregationBatcher>(
    configuration: IntrospectionConfiguration<TAggregationBatcher>,
) where
    TAggregationBatcher: AggregationBatcher + Clone,
    TAggregationBatcher::TBatch: Send,
{
    let gauge_factory = GaugeFactory::default();
    let gauge_future = gauge_factory.clone().report_gauges_forever(
        configuration.cadence,
        configuration.sender.clone(),
        configuration.batcher.clone(),
    );

    INTROSPECTION_GAUGE_FACTORY.store(Some(gauge_factory.into()));

    let _ = gauge_future.await;
    log::warn!("gauge collection ended - terminating introspection metrics");
}

#[allow(unused, clippy::type_complexity)]
pub(crate) fn introspection_factory() -> arc_swap::Guard<Option<Arc<GaugeFactory>>> {
    INTROSPECTION_GAUGE_FACTORY.load()
}
