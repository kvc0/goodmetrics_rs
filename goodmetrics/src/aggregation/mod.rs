//! Types for working with in-memory local aggregations

mod bucket;
mod exponential_histogram;
mod histogram;
mod online_tdigest;
mod statistic_set;
#[allow(clippy::unwrap_used, unused)]
mod tdigest;

pub(crate) use bucket::{bucket_10_2_sigfigs, bucket_10_below_2_sigfigs};
pub use exponential_histogram::ExponentialHistogram;
pub use histogram::Histogram;
pub use online_tdigest::OnlineTdigest;
pub use statistic_set::StatisticSet;
pub use tdigest::{Centroid, TDigest};

// This will need to be reduced. I'm planning to add object pool references
// here; after which this won't be an issue anymore.
#[allow(clippy::large_enum_variant)]
/// For collecting and periodically reporting
#[derive(Debug, Clone)]
pub enum Aggregation {
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
