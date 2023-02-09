use std::collections::HashMap;

use super::bucket::bucket_10_2_sigfigs;

pub type Histogram = HashMap<i64, u64>;

pub trait HistogramAccumulate {
    fn accumulate<T: Into<i64>>(&mut self, value: T);
}
impl HistogramAccumulate for Histogram {
    fn accumulate<T: Into<i64>>(&mut self, value: T) {
        let v = value.into();
        let b = bucket_10_2_sigfigs(v);
        self.insert(b, self[&b] + 1);
    }
}
