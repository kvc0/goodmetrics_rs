use std::{
    cell::UnsafeCell,
    collections::{self, HashMap},
    fmt::Display,
    hash::BuildHasher,
    mem::ManuallyDrop,
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
//
// Metrics objects should not be shared between threads. They are unsynchronized
// and optimized solely for trying to balance overhead cost against observability
// value.
#[derive(Debug)]
pub struct Metrics<TBuildHasher = collections::hash_map::RandomState> {
    pub(crate) metrics_name: Name,
    pub(crate) start_time: Instant,
    dimensions: UnsafeCell<HashMap<Name, Dimension, TBuildHasher>>,
    measurements: UnsafeCell<HashMap<Name, Measurement, TBuildHasher>>,
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
    pub fn dimension(&self, name: impl Into<Name>, value: impl Into<Dimension>) {
        unsafe {
            // SAFETY: within this scope there are no other references to `dimensions`,
            // so ours is effectively unique. I.e., given that there is a reference to self in
            // the function signature, there can be no other mutable reference than that and/or this.
            // We do not release this mutable reference to safe code, and rely on exploiting the
            // following facts for interior mutability safety:
            // * there can be no other mutable reference than possibly the one used to access this function (i.e., self is de-facto &mut).
            // * every unsafe block in Metrics can only be executed synchronously (Metrics are not Sync).
            // * no other code in Metrics can possibly be entered while in an unsafe block.
            let mutable_dimensions = &mut *self.dimensions.get();
            mutable_dimensions.insert(name.into(), value.into());
        }
    }

    #[inline]
    pub fn measurement(&self, name: impl Into<Name>, value: impl Into<Observation>) {
        unsafe {
            // SAFETY: within this scope there are no other references to `measurements`,
            // so ours is effectively unique. I.e., given that there is a reference to self in
            // the function signature, there can be no other mutable reference than that and/or this.
            // We do not release this mutable reference to safe code, and rely on exploiting the
            // following facts for interior mutability safety:
            // * there can be no other mutable reference than possibly the one used to access this function (i.e., self is de-facto &mut).
            // * every unsafe block in Metrics can only be executed synchronously (Metrics are not Sync).
            // * no other code in Metrics can possibly be entered while in an unsafe block.
            let mutable_measurements = &mut *self.measurements.get();
            mutable_measurements.insert(name.into(), Measurement::Observation(value.into()));
        }
    }

    #[inline]
    pub fn distribution(&self, name: impl Into<Name>, value: impl Into<Distribution>) {
        unsafe {
            // SAFETY: within this scope there are no other references to `measurements`,
            // so ours is effectively unique. I.e., given that there is a reference to self in
            // the function signature, there can be no other mutable reference than that and/or this.
            // We do not release this mutable reference to safe code, and rely on exploiting the
            // following facts for interior mutability safety:
            // * there can be no other mutable reference than possibly the one used to access this function (i.e., self is de-facto &mut).
            // * every unsafe block in Metrics can only be executed synchronously (Metrics are not Sync).
            // * no other code in Metrics can possibly be entered while in an unsafe block.
            let mutable_measurements = &mut *self.measurements.get();
            mutable_measurements.insert(name.into(), Measurement::Distribution(value.into()));
        }
    }

    #[inline]
    pub fn time(&self, timer_name: impl Into<Name>) -> Timer<'_, TBuildHasher> {
        Timer::new(self, timer_name)
    }

    #[inline]
    pub fn name(&self) -> &Name {
        &self.metrics_name
    }

    #[inline]
    pub fn restart(&mut self) {
        self.start_time = Instant::now();
        self.dimensions.get_mut().clear();
        self.measurements.get_mut().clear();
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

    /// You should be getting Metrics instances from a MetricsFactory, which will
    /// be set up to send your recordings to wherever they're supposed to go.
    #[inline]
    pub fn new(
        name: impl Into<Name>,
        start_time: Instant,
        dimensions: HashMap<Name, Dimension, TBuildHasher>,
        measurements: HashMap<Name, Measurement, TBuildHasher>,
        behaviors: u32,
    ) -> Self {
        Self {
            metrics_name: name.into(),
            start_time,
            dimensions: UnsafeCell::new(dimensions),
            measurements: UnsafeCell::new(measurements),
            behaviors,
        }
    }

    pub fn drain(
        &mut self,
    ) -> (
        collections::hash_map::Drain<Name, Dimension>,
        collections::hash_map::Drain<Name, Measurement>,
    ) {
        (
            self.dimensions.get_mut().drain(),
            self.measurements.get_mut().drain(),
        )
    }
}

pub struct Timer<'timer, TBuildHasher>
where
    TBuildHasher: BuildHasher,
{
    start_time: Instant,
    metrics: &'timer Metrics<TBuildHasher>,
    name: ManuallyDrop<Name>,
}

impl<'timer, TBuildHasher> Drop for Timer<'timer, TBuildHasher>
where
    TBuildHasher: BuildHasher,
{
    fn drop(&mut self) {
        self.metrics.distribution(
            unsafe { ManuallyDrop::take(&mut self.name) },
            self.start_time.elapsed().as_micros() as u64,
        )
    }
}

impl<'timer, TBuildHasher> Timer<'timer, TBuildHasher>
where
    TBuildHasher: BuildHasher,
{
    pub fn new(metrics: &'timer Metrics<TBuildHasher>, timer_name: impl Into<Name>) -> Self {
        Self {
            start_time: Instant::now(),
            metrics,
            name: ManuallyDrop::new(timer_name.into()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, time::Instant};

    use crate::metrics::{Metrics, Timer};

    fn is_send(_o: impl Send) {}
    // fn is_sync(o: impl Sync) {}

    #[test]
    fn metrics_are_send_but_not_sync() {
        let metrics = Metrics::new(
            "name",
            Instant::now(),
            HashMap::from([]),
            HashMap::from([]),
            0,
        );
        is_send(metrics);
        // Metrics is a single-threaded thing and uses interior mutability. It is Send, but it is not Sync!
        // let metrics = Metrics::new("name", Instant::now(), HashMap::from([]), HashMap::from([]), 0);
        // is_sync(metrics);
    }

    #[test_log::test]
    fn test_timer() {
        let metrics = Metrics::new(
            "name",
            Instant::now(),
            HashMap::from([]),
            HashMap::from([]),
            0,
        );
        let _timer_1 = Timer::new(&metrics, "t1");

        // metrics.dimension("a", "b");
        // let _timer_2 = Timer::new(&mut metrics, "t2");
    }
}
