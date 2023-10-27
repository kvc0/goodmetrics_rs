use std::{
    sync::atomic::{AtomicI64, AtomicU64},
    time::Instant,
};

use crate::pipeline::aggregation::statistic_set::StatisticSet;

/// A guage is a compromise for high throughput metrics. Sometimes you can't afford to
/// allocate a Metrics object to record something, and you can let go of some detail
/// to still be able to record some information. This is the compromise a Gauge allows.
///
/// Gauges are not transactional - you might chop a report just a little bit. If you're
/// using a gauge, you probably require non-blocking behavior more than you require
/// perfect happens-befores in your dashboard data.
///
/// Gauges have internal signed 64 bit integers for sum. This means you can definitely
/// cause a Gauge to roll over if you record a lot of large numbers and/or report
/// infrequently.
pub struct StatisticSetGauge {
    count: AtomicU64,
    sum: AtomicI64,
    min: AtomicI64,
    max: AtomicI64,
}

const ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Relaxed;

pub fn statistic_set_gauge() -> StatisticSetGauge {
    StatisticSetGauge {
        count: AtomicU64::new(0),
        sum: AtomicI64::new(0),
        min: AtomicI64::new(i64::MAX),
        max: AtomicI64::new(i64::MIN),
    }
}

impl StatisticSetGauge {
    /// Observe a value of a gauge explicitly.
    ///
    /// This never blocks. Internal mutability is achieved via platform atomics.
    #[inline]
    pub fn observe(&self, value: impl Into<i64>) {
        let value = value.into();
        self.count.fetch_add(1, ORDERING);
        self.sum.fetch_add(value, ORDERING);
        self.min.fetch_min(value, ORDERING);
        self.max.fetch_max(value, ORDERING);
    }

    /// Observe a value of a gauge explicitly. Note that this can roll over!
    ///
    /// This never blocks. Internal mutability is achieved via platform atomics.
    #[inline]
    pub fn observe_microseconds_since(&self, value: Instant) {
        let value = value.elapsed().as_micros() as i64;
        self.observe(value)
    }
}

impl StatisticSetGauge {
    pub fn reset(&self) -> Option<StatisticSet> {
        let count = self.count.fetch_add(0, ORDERING);
        if count == 0 {
            return None;
        }
        let sum = self.sum.fetch_add(0, ORDERING);
        Some(StatisticSet {
            min: self.min.fetch_max(i64::MAX, ORDERING),
            max: self.max.fetch_min(i64::MIN, ORDERING),
            sum: self.sum.fetch_sub(sum, ORDERING),
            count: self.count.fetch_sub(count, ORDERING),
        })
    }
}
