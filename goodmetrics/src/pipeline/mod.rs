//! Types for wiring to downstream collectors

use std::time::Duration;

use futures::StreamExt;
use futures_batch::ChunksTimeoutStreamExt;

mod aggregator;
mod logging_sink;
mod serializing_sink;
mod stream_sink;

pub use aggregator::{
    AggregatedMetricsMap, Aggregator, DimensionPosition, DimensionedMeasurementsMap,
    DistributionMode, MeasurementAggregationMap, TimeSource,
};
pub use logging_sink::LoggingSink;
pub use serializing_sink::SerializingSink;
pub use stream_sink::StreamSink;

/// A drain that accepts Sunk
pub trait Sink<Sunk> {
    /// Take ownership of a value
    fn accept(&self, to_sink: Sunk);
}

/// Creates size-limited batches of metrics with a batch timeout
pub fn stream_batches<TUpstream, TSunk, FnBatchMap, TBatch>(
    upstream: impl IntoIterator<Item = TSunk>,
    map_batch: FnBatchMap,
    batch_size: usize,
    batch_timeout: Duration,
) -> impl futures::Stream<Item = TBatch>
where
    TUpstream: futures::Stream<Item = TSunk>,
    FnBatchMap: Fn(Vec<TSunk>) -> TBatch,
{
    futures::stream::iter(upstream)
        .chunks_timeout(batch_size, batch_timeout)
        .map(map_batch)
}
