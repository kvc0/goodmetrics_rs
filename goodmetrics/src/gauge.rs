use std::collections::BTreeMap;
use std::{
    sync::atomic::{AtomicI64, AtomicU64},
    time::Instant,
};

use crate::aggregation::{StatisticSet, Sum};
use crate::pipeline::DimensionPosition;
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

/// A gauge is a compromise for high throughput metrics. Sometimes you can't afford to
/// allocate a Metrics object to record something, and you can let go of some detail
/// to still be able to record some information. This is the compromise a Gauge allows.
///
/// Gauges are not transactional - you might chop a report just a little bit. If you're
/// using a gauge, you probably require non-blocking behavior more than you require
/// perfect happens-befores in your dashboard data.
#[derive(Debug)]
pub struct SumGauge {
    sum: AtomicI64,
}

/// A gauge is a compromise for high throughput metrics. Sometimes you can't afford to
/// allocate a Metrics object to record something, and you can let go of some detail
/// to still be able to record some information. This is the compromise a Gauge allows.
///
/// Gauges are not transactional - you might chop a report just a little bit. If you're
/// using a gauge, you probably require non-blocking behavior more than you require
/// perfect happens-befores in your dashboard data.
#[derive(Debug)]
pub enum Gauge {
    /// A statisticset gauge
    StatisticSet(StatisticSetGauge),
    /// A sum gauge
    Sum(SumGauge),
}

impl Gauge {
    /// Observe a value of a gauge.
    ///
    /// This never blocks. Internal mutability is achieved via platform atomics.
    #[inline]
    pub fn observe(&self, value: impl Into<i64>) {
        match self {
            Gauge::StatisticSet(ss) => ss.observe(value),
            Gauge::Sum(s) => s.observe(value),
        }
    }
}

const ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Relaxed;

pub(crate) fn statistic_set_gauge() -> Gauge {
    Gauge::StatisticSet(StatisticSetGauge {
        count: AtomicU64::new(0),
        sum: AtomicI64::new(0),
        min: AtomicI64::new(i64::MAX),
        max: AtomicI64::new(i64::MIN),
    })
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

    /// Takes a dirty snapshot of the gauge without locking.
    /// This is susceptible to toctou, and the intent is to only have 1 thread
    /// calling reset() as part of metrics reporting.
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

pub(crate) fn sum_gauge() -> Gauge {
    Gauge::Sum(SumGauge {
        sum: AtomicI64::new(0),
    })
}

impl SumGauge {
    /// Observe a value of a gauge explicitly.
    ///
    /// This never blocks. Internal mutability is achieved via platform atomics.
    #[inline]
    pub fn observe(&self, count: impl Into<i64>) {
        self.sum.fetch_add(count.into(), ORDERING);
    }

    /// Takes a dirty snapshot of the gauge without locking.
    /// This is susceptible to toctou, and the intent is to only have 1 thread
    /// calling reset() as part of metrics reporting.
    pub fn reset(&self) -> Option<Sum> {
        let sum = self.sum.swap(0, ORDERING);

        if sum == 0 {
            return None;
        }

        Some(Sum { sum })
    }
}

/// A GaugeDimensions is a group of dimensions to be used with a gauge.
/// It is consumed when the gauge is created.
#[derive(Debug, Default, Clone)]
pub struct GaugeDimensions {
    dimension_position: DimensionPosition,
}

impl GaugeDimensions {
    /// Create a new GaugeDimensions
    ///
    /// ```
    /// # use goodmetrics::GaugeDimensions;
    /// GaugeDimensions::new([("a", "dimension"), ("another", "dimension")]);
    /// ```
    pub fn new(
        dimensions: impl IntoIterator<Item = (impl Into<Name>, impl Into<Dimension>)>,
    ) -> Self {
        Self {
            dimension_position: BTreeMap::from_iter(
                dimensions
                    .into_iter()
                    .map(|(name, dimension)| (name.into(), dimension.into())),
            ),
        }
    }

    /// Add a name/dimension to the GaugeDimensions.
    /// Can be chained for successive inserts.
    ///
    /// ```
    /// use goodmetrics::GaugeDimensions;
    ///
    /// let mut dimensions = GaugeDimensions::default();
    /// dimensions.insert("a", "dimension");
    /// dimensions.insert("another", "dimension");
    /// ```
    pub fn insert(&mut self, name: impl Into<Name>, dimension: impl Into<Dimension>) -> &mut Self {
        self.dimension_position
            .insert(name.into(), dimension.into());
        self
    }
}

impl From<GaugeDimensions> for DimensionPosition {
    fn from(value: GaugeDimensions) -> Self {
        value.dimension_position
    }
}
