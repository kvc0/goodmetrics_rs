use std::collections::BTreeMap;
use std::{
    sync::atomic::{AtomicI64, AtomicU64},
    time::Instant,
};

use crate::pipeline::aggregation::statistic_set::StatisticSet;
use crate::pipeline::aggregator::DimensionPosition;
use crate::types::{Dimension, Name};

/// A gauge is a compromise for high throughput metrics. Sometimes you can't afford to
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
#[derive(Debug)]
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

/// A GaugeDimensions is a group of dimensions to be used with a gauge.
/// It is consumed when the gauge is created.
#[derive(Debug, Default)]
pub struct GaugeDimensions {
    pub dimension_position: DimensionPosition,
}

impl GaugeDimensions {
    /// Create a new, empty, GaugeDimensions
    pub fn new() -> Self {
        Self {
            dimension_position: Default::default(),
        }
    }

    /// Create a new GaugeDimensions from a given name and dimension
    ///
    /// ```
    /// use goodmetrics::gauge::GaugeDimensions;
    ///
    /// GaugeDimensions::from("a", "dimension");
    /// ```
    pub fn from(name: impl Into<Name>, dimension: impl Into<Dimension>) -> Self {
        Self {
            dimension_position: BTreeMap::from([(name.into(), dimension.into())]),
        }
    }

    /// Add a name/dimension to the GaugeDimensions.
    /// Can be chained for successive inserts.
    ///
    /// ```
    /// use goodmetrics::gauge::GaugeDimensions;
    ///
    /// let mut dimensions = GaugeDimensions::new();
    /// dimensions.insert("a", "dimension");
    /// dimensions.insert("another", "dimension");
    /// ```
    pub fn insert(&mut self, name: impl Into<Name>, dimension: impl Into<Dimension>) -> &mut Self {
        self.dimension_position
            .insert(name.into(), dimension.into());
        self
    }

    /// Add a name/dimension to the GaugeDimensions, taking and returning ownership of self.
    /// For chaining in value positions without an intermediate `let`
    ///
    /// ```
    /// use goodmetrics::gauge::GaugeDimensions;
    ///
    /// GaugeDimensions::from("a", "dimension").with_dimension("another", "dimension");
    /// ```
    pub fn with_dimension(
        mut self,
        name: impl Into<Name>,
        dimension: impl Into<Dimension>,
    ) -> Self {
        self.dimension_position
            .insert(name.into(), dimension.into());
        self
    }
}
