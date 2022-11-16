use std::time::Duration;

use futures::StreamExt;
use futures_batch::ChunksTimeoutStreamExt;

pub mod aggregating_sink;
pub mod logging_sink;
pub mod serializing_sink;

pub trait Sink<Sunk> {
    fn accept(&self, to_sink: Sunk);
}

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
