use std::{
    collections::HashMap,
    fmt::Display,
    hash::BuildHasher,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    time::Instant,
};

use crate::{
    allocator::Hasher,
    types::{Dimension, Distribution, Measurement, Name, Observation},
};

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
pub struct Metrics<TBuildHasher = Hasher> {
    pub(crate) metrics_name: Name,
    pub(crate) start_time: Instant,
    dimensions: HashMap<Name, Dimension, TBuildHasher>,
    measurements: HashMap<Name, Measurement, TBuildHasher>,
    dimension_guards: Vec<OverrideDimension>,
    pub(crate) behaviors: u32,
}

#[derive(Debug)]
pub struct DimensionGuard {
    value: Arc<Mutex<Dimension>>,
}
impl DimensionGuard {
    fn new(name: Name, default: Dimension) -> (Self, OverrideDimension) {
        let value = Arc::new(Mutex::new(default));
        (
            Self {
                value: value.clone(),
            },
            OverrideDimension { name, value },
        )
    }

    /// Track how far you got without a final result
    pub fn breadcrumb(&self, dimension: impl Into<Dimension>) {
        *self.value.lock().expect("local mutex") = dimension.into()
    }

    /// Set the final result of the dimension
    pub fn set(self, dimension: impl Into<Dimension>) {
        *self.value.lock().expect("local mutex") = dimension.into()
    }
}

#[derive(Debug)]
pub struct OverrideDimension {
    name: Name,
    value: Arc<Mutex<Dimension>>,
}

impl OverrideDimension {
    /// Consume the dimension. If it hasn't been set by now, you get the default value.
    pub fn redeem(self) -> (Name, Dimension) {
        let dimension = std::mem::replace(
            &mut *self.value.lock().expect("local mutex"),
            "__none__".into(),
        );
        (self.name, dimension)
    }
}

impl<H> AsRef<Metrics<H>> for Metrics<H> {
    fn as_ref(&self) -> &Metrics<H> {
        self
    }
}

impl<H> AsMut<Metrics<H>> for Metrics<H> {
    fn as_mut(&mut self) -> &mut Metrics<H> {
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
    pub fn dimension(&mut self, name: impl Into<Name>, value: impl Into<Dimension>) {
        self.dimensions.insert(name.into(), value.into());
    }

    /// Record a measurement name and value pair - last write per metrics object wins!
    #[inline]
    pub fn measurement(&mut self, name: impl Into<Name>, value: impl Into<Observation>) {
        self.measurements
            .insert(name.into(), Measurement::Observation(value.into()));
    }

    /// Record a distribution name and value pair - last write per metrics object wins!
    /// Check out t-digests if you're using a goodmetrics + timescale downstream.
    #[inline]
    pub fn distribution(&mut self, name: impl Into<Name>, value: impl Into<Distribution>) {
        self.measurements
            .insert(name.into(), Measurement::Distribution(value.into()));
    }

    /// Record a time distribution in nanoseconds.
    /// Check out t-digests if you're using a goodmetrics + timescale downstream.
    ///
    /// The returned Timer is a scope guard.
    #[inline]
    pub fn time(&mut self, timer_name: impl Into<Name>) -> Timer {
        let timer = Arc::new(AtomicUsize::new(0));
        self.measurements.insert(
            timer_name.into(),
            Measurement::Distribution(Distribution::Timer {
                nanos: timer.clone(),
            }),
        );
        Timer::new(timer)
    }

    /// A dimension that you set a default for in case you drop early or something.
    pub fn guarded_dimension(
        &mut self,
        name: impl Into<Name>,
        default: impl Into<Dimension>,
    ) -> DimensionGuard {
        let (guard, dimension) = DimensionGuard::new(name.into(), default.into());
        self.dimension_guards.push(dimension);
        guard
    }

    /// Name of the metrics you passed in when you created it.
    #[inline]
    pub fn name(&self) -> &Name {
        &self.metrics_name
    }

    /// Clear the structure in preparation for reuse without allocation.
    /// You still need to set the right behaviors and start times.
    #[inline]
    pub fn restart(&mut self) {
        self.dimensions.clear();
        self.measurements.clear();
        self.dimension_guards.clear();
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
        dimensions: HashMap<Name, Dimension, TBuildHasher>,
        measurements: HashMap<Name, Measurement, TBuildHasher>,
        dimension_guards: Vec<OverrideDimension>,
        behaviors: u32,
    ) -> Self {
        Self {
            metrics_name: name.into(),
            start_time,
            dimensions,
            measurements,
            behaviors,
            dimension_guards,
        }
    }

    /// A sibling of restart(), this is a way to destructively move dimensions and
    /// measurements out of the metrics for aggregation or downstream message building.
    pub fn drain(
        &mut self,
    ) -> (
        &mut HashMap<Name, Dimension, TBuildHasher>,
        &mut HashMap<Name, Measurement, TBuildHasher>,
    ) {
        while let Some(d) = self.dimension_guards.pop() {
            let (name, value) = d.redeem();
            self.dimension(name, value);
        }
        (&mut self.dimensions, &mut self.measurements)
    }
}

/// Scope guard for recording nanoseconds into a Metrics.
/// Starts recording when you create it.
/// Stops recording and puts its measurement into the Metrics as a distribution when you drop it.
pub struct Timer {
    start_time: Instant,
    timer: Arc<AtomicUsize>,
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.timer.store(
            self.start_time.elapsed().as_nanos() as usize,
            std::sync::atomic::Ordering::Release,
        );
    }
}

impl Timer {
    pub fn new(timer: Arc<AtomicUsize>) -> Self {
        Self {
            start_time: Instant::now(),
            timer,
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, time::Instant};

    use crate::metrics::Metrics;

    fn is_send(_o: impl Send) {}
    fn is_sync(_o: impl Sync) {}

    #[test]
    fn metrics_are_send_and_sync() {
        let metrics = Metrics::new(
            "name",
            Instant::now(),
            HashMap::from([]),
            HashMap::from([]),
            Vec::new(),
            0,
        );
        is_send(metrics);

        let metrics = Metrics::new(
            "name",
            Instant::now(),
            HashMap::from([]),
            HashMap::from([]),
            Vec::new(),
            0,
        );
        is_sync(metrics);
    }

    #[test_log::test]
    fn test_timer() {
        let mut metrics = Metrics::new(
            "name",
            Instant::now(),
            HashMap::from([]),
            HashMap::from([]),
            Vec::new(),
            0,
        );
        let timer_1 = metrics.time("t1");
        is_send(timer_1);
        let timer_1 = metrics.time("t1");
        is_sync(timer_1);
    }
}
