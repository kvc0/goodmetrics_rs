use self::{
    exponential_histogram::ExponentialHistogram, histogram::Histogram,
    online_tdigest::OnlineTdigest, statistic_set::StatisticSet,
};

pub mod bucket;
pub mod exponential_histogram;
pub mod histogram;
pub mod online_tdigest;
pub mod statistic_set;
#[allow(clippy::unwrap_used)]
pub mod tdigest;

// This will need to be reduced. I'm planning to add object pool references
// here; after which this won't be an issue anymore.
#[allow(clippy::large_enum_variant)]
/// For collecting and periodically reporting
#[derive(Debug, Clone)]
pub enum Aggregation {
    ExponentialHistogram(ExponentialHistogram),
    Histogram(Histogram),
    StatisticSet(StatisticSet),
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

impl Aggregation {
    /// Reset the aggregation to an empty initial state
    pub fn zero(&mut self) {
        match self {
            Aggregation::ExponentialHistogram(e) => e.zero(),
            Aggregation::Histogram(h) => h.clear(),
            Aggregation::StatisticSet(s) => s.zero(),
            Aggregation::TDigest(t) => {
                t.reset_mut();
            }
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Aggregation::ExponentialHistogram(e) => e.is_empty(),
            Aggregation::Histogram(h) => h.is_empty(),
            Aggregation::StatisticSet(s) => s.is_empty(),
            Aggregation::TDigest(t) => t.is_empty(),
        }
    }
}
