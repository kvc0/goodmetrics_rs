use std::{collections::HashMap, time::Duration};

use futures::StreamExt;
use futures_batch::ChunksTimeoutStreamExt;

use crate::types::{self, Distribution};

use self::aggregation::{bucket::bucket_10_2_sigfigs, online_tdigest::OnlineTdigest};

pub mod aggregation;
pub mod aggregator;
pub mod logging_sink;
pub mod serializing_sink;
pub mod stream_sink;

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

pub trait AbsorbDistribution {
    fn absorb(&mut self, distribution: Distribution);
}

impl AbsorbDistribution for HashMap<i64, u64> {
    fn absorb(&mut self, distribution: Distribution) {
        match distribution {
            types::Distribution::I64(i) => {
                self.entry(bucket_10_2_sigfigs(i))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            types::Distribution::I32(i) => {
                self.entry(bucket_10_2_sigfigs(i.into()))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            types::Distribution::U64(i) => {
                self.entry(bucket_10_2_sigfigs(i as i64))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            types::Distribution::U32(i) => {
                self.entry(bucket_10_2_sigfigs(i.into()))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            types::Distribution::Collection(collection) => {
                collection.iter().for_each(|i| {
                    self.entry(bucket_10_2_sigfigs(*i))
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                });
            }
            types::Distribution::Timer { nanos } => {
                let v = nanos.load(std::sync::atomic::Ordering::Acquire);
                self.entry(bucket_10_2_sigfigs(v as i64))
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        };
    }
}

impl AbsorbDistribution for OnlineTdigest {
    fn absorb(&mut self, distribution: Distribution) {
        match distribution {
            types::Distribution::I64(i) => self.observe_mut(i as f64),
            types::Distribution::I32(i) => self.observe_mut(i),
            types::Distribution::U64(i) => self.observe_mut(i as f64),
            types::Distribution::U32(i) => self.observe_mut(i),
            types::Distribution::Collection(collection) => {
                collection.iter().for_each(|i| self.observe_mut(*i as f64));
            }
            types::Distribution::Timer { nanos } => {
                self.observe_mut(nanos.load(std::sync::atomic::Ordering::Acquire) as f64)
            }
        };
    }
}
