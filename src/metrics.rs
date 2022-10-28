use std::{
    collections::{self, HashMap},
    hash::BuildHasher,
    time::Instant,
};

use crate::types::{Dimension, Distribution, Measurement, Name, Observation};

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
    metrics_name: Name,
    start_time: Instant,
    dimensions: HashMap<Name, Dimension, TBuildHasher>,
    measurements: HashMap<Name, Measurement, TBuildHasher>,
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
    pub fn restart(&mut self) {
        self.start_time = Instant::now();
        self.dimensions.clear();
        self.measurements.clear();
    }

    #[inline]
    pub(crate) fn new(
        name: impl Into<Name>,
        start_time: Instant,
        dimensions: HashMap<Name, Dimension, TBuildHasher>,
        measurements: HashMap<Name, Measurement, TBuildHasher>,
    ) -> Self {
        Self {
            metrics_name: name.into(),
            start_time,
            dimensions,
            measurements,
        }
    }
}
