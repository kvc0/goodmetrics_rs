use std::collections::HashMap;

use super::bucket::bucket_10_2_sigfigs;

/// A straightforward histogram with buckets and counts.
/// You should use a consistent bucket strategy, like tenths-of-powers-of-ten.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub(crate) histogram: HashMap<i64, u64>,
}

impl Histogram {
    /// Add 1 to the value's bucket
    pub fn accumulate<T: Into<i64>>(&mut self, value: T) {
        let v = value.into();
        let bucket = bucket_10_2_sigfigs(v);
        self.histogram
            .entry(bucket)
            .and_modify(|b| *b += 1)
            .or_insert(1);
    }

    /// Consume this histogram into a map of threshold -> count
    pub fn into_map(self) -> HashMap<i64, u64> {
        self.histogram
    }
}
