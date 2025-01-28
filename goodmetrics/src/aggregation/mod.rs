//! Types for working with in-memory local aggregations

mod bucket;
mod histogram;
mod online_tdigest;
mod statistic_set;
mod sum;
#[allow(clippy::unwrap_used, unused)]
mod tdigest;

pub(crate) use bucket::bucket_10_below_2_sigfigs;
use exponential_histogram::ExponentialHistogram;
pub use histogram::Histogram;
pub use online_tdigest::OnlineTdigest;
pub use statistic_set::StatisticSet;
pub use sum::Sum;
pub use tdigest::{Centroid, TDigest};

use crate::types::Distribution;

// This will need to be reduced. I'm planning to add object pool references
// here; after which this won't be an issue anymore.
#[allow(clippy::large_enum_variant)]
/// For collecting and periodically reporting
#[derive(Debug, Clone)]
pub enum Aggregation {
    /// A sum
    Sum(Sum),
    /// An exponential histogram aggregation
    ExponentialHistogram(ExponentialHistogram),
    /// A tenths-of-base-10 histogram aggregation
    Histogram(Histogram),
    /// A min/max/sum/count aggregation
    StatisticSet(StatisticSet),
    /// A t-digest aggregation
    TDigest(OnlineTdigest),
}

impl PartialEq for Aggregation {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ExponentialHistogram(l), Self::ExponentialHistogram(r)) => l == r,
            (Self::Histogram(l), Self::Histogram(r)) => l == r,
            (Self::StatisticSet(l), Self::StatisticSet(r)) => l == r,
            _ => false,
        }
    }
}

impl std::fmt::Display for Aggregation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Aggregation::Sum(sum) => sum.fmt(f),
            Aggregation::StatisticSet(ss) => ss.fmt(f),
            Aggregation::ExponentialHistogram(eh) => eh.fmt(f),
            Aggregation::Histogram(h) => h.fmt(f),
            Aggregation::TDigest(td) => td.fmt(f),
        }
    }
}

/// Ability to accept Distributions into a structure
pub trait AbsorbDistribution {
    /// Absorb each value of a distribution into a structure
    fn absorb(&mut self, distribution: Distribution);
}

impl AbsorbDistribution for Histogram {
    fn absorb(&mut self, distribution: Distribution) {
        match distribution {
            Distribution::I64(i) => {
                self.accumulate(i);
            }
            Distribution::I32(i) => {
                self.accumulate(i);
            }
            Distribution::U64(i) => {
                self.accumulate(i as i64);
            }
            Distribution::U32(i) => {
                self.accumulate(i);
            }
            Distribution::Collection(collection) => {
                collection.iter().for_each(|i| {
                    self.accumulate(*i);
                });
            }
            Distribution::Timer { nanos } => {
                let v = nanos.load(std::sync::atomic::Ordering::Acquire);
                self.accumulate(v as i64);
            }
        };
    }
}

impl AbsorbDistribution for OnlineTdigest {
    fn absorb(&mut self, distribution: Distribution) {
        match distribution {
            Distribution::I64(i) => self.observe_mut(i as f64),
            Distribution::I32(i) => self.observe_mut(i),
            Distribution::U64(i) => self.observe_mut(i as f64),
            Distribution::U32(i) => self.observe_mut(i),
            Distribution::Collection(collection) => {
                collection.iter().for_each(|i| self.observe_mut(*i as f64));
            }
            Distribution::Timer { nanos } => {
                self.observe_mut(nanos.load(std::sync::atomic::Ordering::Acquire) as f64)
            }
        };
    }
}

impl AbsorbDistribution for ExponentialHistogram {
    fn absorb(&mut self, distribution: crate::types::Distribution) {
        match distribution {
            Distribution::I64(i) => self.accumulate(i as f64),
            Distribution::I32(i) => self.accumulate(i),
            Distribution::U64(u) => self.accumulate(u as f64),
            Distribution::U32(u) => self.accumulate(u),
            Distribution::Collection(c) => {
                for i in c {
                    self.accumulate(i as f64)
                }
            }
            Distribution::Timer { nanos } => {
                self.accumulate(nanos.load(std::sync::atomic::Ordering::Relaxed) as f64)
            }
        }
    }
}
