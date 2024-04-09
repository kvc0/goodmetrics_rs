use std::cmp::{max, min};

/// A basic aggregation.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct StatisticSet {
    /// Minimum observed value
    pub min: i64,
    /// Maximum observed value
    pub max: i64,
    /// Sum of all observed values
    pub sum: i64,
    /// Count of observations
    pub count: u64,
}

impl Default for StatisticSet {
    fn default() -> Self {
        Self {
            min: i64::MAX,
            max: i64::MIN,
            sum: 0,
            count: 0,
        }
    }
}

impl StatisticSet {
    pub(crate) fn accumulate<T: Into<i64>>(&mut self, value: T) {
        let v: i64 = value.into();
        self.min = min(v, self.min);
        self.max = max(v, self.max);
        self.sum += v;
        self.count += 1;
    }
}
