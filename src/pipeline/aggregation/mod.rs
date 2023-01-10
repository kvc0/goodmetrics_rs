use self::{histogram::Histogram, statistic_set::StatisticSet};

pub mod bucket;
pub mod histogram;
pub mod statistic_set;

// For collecting and periodically reporting
#[derive(Debug, PartialEq, Eq)]
pub enum Aggregation {
    Histogram(Histogram),
    StatisticSet(StatisticSet),
    //    TDigest(OnlineTdigest),
}
