use std::{
    collections::{self, BTreeMap, HashMap},
    fmt::Display,
    hash::BuildHasher,
    mem::ManuallyDrop,
    sync::Mutex,
    time::Instant,
};

use crate::types::{Dimension, Distribution, Measurement, Name, Observation};

#[derive(Clone, Copy, Debug)]
pub enum MetricsBehavior {
    Default = 0x00000000,
    SuppressTotalTime = 0x00000001,
    Suppress = 0x00000010,
}

/// A Metrics encapsulates a unit of work.
///
/// It is a record of the interesting things that happened during that unit,
/// and the things that describe it.
///
/// A web request handler, grpc service rpc implementation or periodic job's
/// execution is a unit of work you should have a Metrics for.
///
/// Metrics does not deal in things like "gauges" or "counters." It is more
/// like a single-level trace; it works with concrete observations rather than
/// exposing your code to aggregators.
///
/// Metrics objects are emitted through a reporter chain when they are Dropped.
/// It is at that point that aggregation, if you've configured any, is performed.
///
/// Your code is responsible for putting the details of interest into the
/// Metrics object as it encounters interesting details. You just record what you
/// want to.
///
/// Generally prefer to record dimensions as early as possible, in case you
/// early-out somewhere; in this way you'll have more information in those cases.
#[derive(Debug)]
pub struct Metrics<TBuildHasher = collections::hash_map::RandomState> {
    pub(crate) metrics_name: Name,
    pub(crate) start_time: Instant,
    dimensions: Mutex<BTreeMap<Name, Dimension>>,
    measurements: Mutex<HashMap<Name, Measurement, TBuildHasher>>,
    pub(crate) behaviors: u32,
}

impl AsRef<Metrics> for Metrics {
    fn as_ref(&self) -> &Metrics {
        self
    }
}

impl AsMut<Metrics> for Metrics {
    fn as_mut(&mut self) -> &mut Metrics {
        self
    }
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
    /// Record a dimension name and value pair - last write per metrics object wins!
    #[inline]
    pub fn dimension(&self, name: impl Into<Name>, value: impl Into<Dimension>) {
        let mut mutable_dimensions = self.dimensions.lock().expect("Mutex was unable to lock!");
        mutable_dimensions.insert(name.into(), value.into());
    }

    /// Record a dimension name and value pair - last write per metrics object wins!
    /// Prefer this when you have mut on Metrics. It's faster!
    #[inline]
    pub fn dimension_mut(&mut self, name: impl Into<Name>, value: impl Into<Dimension>) {
        let mutable_dimensions = self
            .dimensions
            .get_mut()
            .expect("Mutex was unable to lock!");
        mutable_dimensions.insert(name.into(), value.into());
    }

    /// Record a measurement name and value pair - last write per metrics object wins!
    #[inline]
    pub fn measurement(&self, name: impl Into<Name>, value: impl Into<Observation>) {
        let mut mutable_measurements = self.measurements.lock().expect("Mutex was unable to lock!");
        mutable_measurements.insert(name.into(), Measurement::Observation(value.into()));
    }

    /// Record a measurement name and value pair - last write per metrics object wins!
    /// Prefer this when you have mut on Metrics. It's faster!
    #[inline]
    pub fn measurement_mut(&mut self, name: impl Into<Name>, value: impl Into<Observation>) {
        let mutable_measurements = self
            .measurements
            .get_mut()
            .expect("Mutex was unable to lock!");
        mutable_measurements.insert(name.into(), Measurement::Observation(value.into()));
    }

    /// Record a distribution name and value pair - last write per metrics object wins!
    /// Check out t-digests if you're using a goodmetrics + timescale downstream.
    #[inline]
    pub fn distribution(&self, name: impl Into<Name>, value: impl Into<Distribution>) {
        let mut mutable_measurements = self.measurements.lock().expect("Mutex was unable to lock!");
        mutable_measurements.insert(name.into(), Measurement::Distribution(value.into()));
    }

    /// Record a distribution name and value pair - last write per metrics object wins!
    /// Prefer this when you have mut on Metrics. It's faster!
    /// Check out t-digests if you're using a goodmetrics + timescale downstream.
    #[inline]
    pub fn distribution_mut(&mut self, name: impl Into<Name>, value: impl Into<Distribution>) {
        let mutable_measurements = self
            .measurements
            .get_mut()
            .expect("Mutex was unable to lock!");
        mutable_measurements.insert(name.into(), Measurement::Distribution(value.into()));
    }

    /// Record a time distribution in nanoseconds.
    /// Check out t-digests if you're using a goodmetrics + timescale downstream.
    ///
    /// The returned Timer is a scope guard.
    #[inline]
    pub fn time(&self, timer_name: impl Into<Name>) -> Timer<'_, TBuildHasher> {
        Timer::new(self, timer_name)
    }

    /// Name of the metrics you passed in when you created it.
    #[inline]
    pub fn name(&self) -> &Name {
        &self.metrics_name
    }

    /// Clear the structure in preparation for reuse without allocation.
    #[inline]
    pub fn restart(&mut self) {
        self.start_time = Instant::now();
        self.dimensions
            .get_mut()
            .expect("Mutex was unable to lock!")
            .clear();
        self.measurements
            .get_mut()
            .expect("Mutex was unable to lock!")
            .clear();
    }

    /// Do not report this metrics instance.
    pub fn suppress(&mut self) {
        self.behaviors |= MetricsBehavior::Suppress as u32;
    }

    /// Check if this metrics has that behavior.
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
        dimensions: BTreeMap<Name, Dimension>,
        measurements: HashMap<Name, Measurement, TBuildHasher>,
        behaviors: u32,
    ) -> Self {
        Self {
            metrics_name: name.into(),
            start_time,
            dimensions: Mutex::new(dimensions),
            measurements: Mutex::new(measurements),
            behaviors,
        }
    }

    /// A sibling of restart(), this is a way to destructively move dimensions and
    /// measurements out of the metrics for aggregation or downstream message building.
    pub fn drain(
        &mut self,
    ) -> (
        &mut BTreeMap<Name, Dimension>,
        &mut HashMap<Name, Measurement, TBuildHasher>,
    ) {
        (
            self.dimensions
                .get_mut()
                .expect("Mutex was unable to lock!"),
            self.measurements
                .get_mut()
                .expect("Mutex was unable to lock!"),
        )
    }
}

/// Scope guard for recording nanoseconds into a Metrics.
/// Starts recording when you create it.
/// Stops recording and puts its measurement into the Metrics as a distribution when you drop it.
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
            self.start_time.elapsed(),
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
    use std::{
        collections::{BTreeMap, HashMap},
        time::Instant,
    };

    use crate::metrics::{Metrics, Timer};

    fn is_send(_o: impl Send) {}
    fn is_sync(_o: impl Sync) {}

    #[test]
    fn metrics_are_send_and_sync() {
        let metrics = Metrics::new(
            "name",
            Instant::now(),
            BTreeMap::from([]),
            HashMap::from([]),
            0,
        );
        is_send(metrics);

        let metrics = Metrics::new(
            "name",
            Instant::now(),
            BTreeMap::from([]),
            HashMap::from([]),
            0,
        );
        is_sync(metrics);
    }

    #[test_log::test]
    fn test_timer() {
        let metrics = Metrics::new(
            "name",
            Instant::now(),
            BTreeMap::from([]),
            HashMap::from([]),
            0,
        );
        let timer_1 = Timer::new(&metrics, "t1");
        is_send(timer_1);
        let timer_1 = Timer::new(&metrics, "t1");
        is_sync(timer_1);
    }
}
