use self::{histogram::Histogram, online_tdigest::OnlineTdigest, statistic_set::StatisticSet};

pub mod bucket;
pub mod histogram;
pub mod online_tdigest;
pub mod statistic_set;
#[allow(clippy::unwrap_used)] // this vendored code is unwrap-happy
pub mod tdigest;

// This will need to be reduced. I'm planning to add object pool references
// here; after which this won't be an issue anymore.
#[allow(clippy::large_enum_variant)]
/// For collecting and periodically reporting
#[derive(Debug)]
pub enum Aggregation {
    Histogram(Histogram),
    StatisticSet(StatisticSet),
    TDigest(OnlineTdigest),
}

impl PartialEq for Aggregation {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Histogram(l), Self::Histogram(r)) => l == r,
            (Self::StatisticSet(l), Self::StatisticSet(r)) => l == r,
            _ => false,
        }
    }
}
