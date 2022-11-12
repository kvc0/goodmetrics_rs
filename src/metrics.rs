use std::{
    collections::{self, HashMap},
    fmt::Display,
    hash::BuildHasher,
    time::Instant,
};

use crate::types::{Dimension, Distribution, Measurement, Name, Observation};

pub enum MetricsBehavior {
    Default = 0x00000000,
    SuppressTotalTime = 0x00000001,
    Suppress = 0x00000010,
}

// A Metrics encapsulates 1 unit of work.
// It is a record of the interesting things that happened during that work.
// A web request handler is a unit of work.
// A periodic job's execution is a unit of work.
//
// Metrics does not deal in things like "gauges" or "counters." It concerns
// itself with concrete, unary observations - like your code does.
//
// Metrics objects are emitted through a reporter chain when they are Dropped.
// It is at that point that aggregation, if any, is performed.
//
// Your code is responsible for putting the details of interest into the
// Metrics object as it encounters interesting details. You do not need to
// structure anything specially for Metrics. You just record what you want to.
#[derive(Debug)]
pub struct Metrics<TBuildHasher = collections::hash_map::RandomState> {
    pub(crate) metrics_name: Name,
    pub(crate) start_time: Instant,
    pub(crate) dimensions: HashMap<Name, Dimension, TBuildHasher>,
    pub(crate) measurements: HashMap<Name, Measurement, TBuildHasher>,
    pub(crate) behaviors: u32,
}

// Blanket implementation for any kind of metrics - T doesn't factor into the display
impl<T> Display for Metrics<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{name}: {behaviors:#04b} {time:#?}, dimensions: {dimensions:#?}, measurements: {measurements:#?}",
            name=self.metrics_name,
            time=self.start_time,
            behaviors=self.behaviors,
            dimensions=self.dimensions,
            measurements=self.measurements
        )
    }
}

impl<TBuildHasher> Metrics<TBuildHasher>
where
    TBuildHasher: BuildHasher,
{
    #[inline]
    pub fn dimension(&mut self, name: impl Into<Name>, value: impl Into<Dimension>) {
        self.dimensions.insert(name.into(), value.into());
    }

    #[inline]
    pub fn measurement(&mut self, name: impl Into<Name>, value: impl Into<Observation>) {
        self.measurements
            .insert(name.into(), Measurement::Observation(value.into()));
    }

    #[inline]
    pub fn distribution(&mut self, name: impl Into<Name>, value: impl Into<Distribution>) {
        self.measurements
            .insert(name.into(), Measurement::Distribution(value.into()));
    }

    #[inline]
    pub fn name(&self) -> &Name {
        &self.metrics_name
    }

    #[inline]
    pub fn restart(&mut self) {
        self.start_time = Instant::now();
        self.dimensions.clear();
        self.measurements.clear();
    }

    /// do not report this metrics instance
    pub fn suppress(&mut self) {
        self.behaviors |= MetricsBehavior::Suppress as u32;
    }

    #[inline]
    pub fn has_behavior(&self, behavior: MetricsBehavior) -> bool {
        0 != self.behaviors & behavior as u32
    }

    /// # Safety
    ///
    /// This function is intended to be used by MetricsFactories while creating
    /// new instances. It is not intended for use outside of infrastructure code.
    /// It is exposed in case you have something special you need to do with your
    /// allocator.
    /// You shouldn't call this unless you know you need to and provide your own
    /// guarantees about when the behavior is added and whether it's legal & valid
    #[inline]
    pub unsafe fn add_behavior(&mut self, behavior: MetricsBehavior) {
        self.set_raw_behavior(behavior as u32)
    }

    /// # Safety
    ///
    /// This function is intended to be used by MetricsFactories while creating
    /// new instances. It is not intended for use outside of infrastructure code.
    /// It is exposed in case you have something special you need to do with your
    /// allocator.
    /// You shouldn't call this unless you know you need to and provide your own
    /// guarantees about when the behavior is added and whether it's legal & valid
    #[inline]
    pub unsafe fn set_raw_behavior(&mut self, behavior: u32) {
        self.behaviors |= behavior
    }

    #[inline]
    pub(crate) fn new(
        name: impl Into<Name>,
        start_time: Instant,
        dimensions: HashMap<Name, Dimension, TBuildHasher>,
        measurements: HashMap<Name, Measurement, TBuildHasher>,
        behaviors: u32,
    ) -> Self {
        Self {
            metrics_name: name.into(),
            start_time,
            dimensions,
            measurements,
            behaviors,
        }
    }
}
