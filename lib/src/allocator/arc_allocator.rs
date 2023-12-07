use crate::{
    metrics::{Metrics, MetricsBehavior},
    types::Name,
};
use std::{
    cmp::max,
    collections::HashMap,
    hash::BuildHasher,
    mem::take,
    ops::{Deref, DerefMut},
    sync::{atomic::AtomicUsize, Arc, Mutex},
    time::Instant,
};

use super::MetricsAllocator;

/// A metrics allocator that prioritizes reusing Metrics objects, but with a gentler
/// Borrow Checker constraint. References are owned, so it's easier to pass them
/// around.
#[derive(Clone)]
pub struct ArcAllocator<TBuildHasher: Send> {
    state: Arc<AllocatorState<TBuildHasher>>,
}

struct AllocatorState<TBuildHasher: Send> {
    metrics_cache: [Mutex<Vec<Metrics<TBuildHasher>>>; 8],
    cache_clock: AtomicUsize,
}

pub type ArcMetrics<TBuildHasher> = Arc<Metrics<TBuildHasher>>;

impl<TBuildHasher> ArcAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    pub fn new(cache_size: usize) -> Self {
        Self {
            state: Arc::new(AllocatorState {
                metrics_cache: [(); 8]
                    .map(|_| Mutex::new(Vec::with_capacity(max(16, cache_size / 8)))),
                cache_clock: Default::default(),
            }),
        }
    }
}

impl<'a, TBuildHasher: Send> MetricsAllocator<'a, CachedMetrics<TBuildHasher>>
    for ArcAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + 'a,
{
    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> CachedMetrics<TBuildHasher> {
        let slot = self
            .state
            .cache_clock
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.state.metrics_cache.len();
        let metrics = self.state.metrics_cache[slot]
            .lock()
            .expect("local mutex")
            .pop();
        let metrics = match metrics {
            Some(mut reused_metrics) => {
                reused_metrics.restart();
                reused_metrics.metrics_name = metrics_name.into();
                reused_metrics.start_time = Instant::now();
                reused_metrics.behaviors = MetricsBehavior::Default as u32;
                reused_metrics
            }
            None => Metrics::new(
                metrics_name,
                Instant::now(),
                HashMap::with_hasher(Default::default()),
                HashMap::with_hasher(Default::default()),
                Vec::new(),
                MetricsBehavior::Default as u32,
            ),
        };
        CachedMetrics {
            allocator: self.state.clone(),
            metrics: Some(metrics),
        }
    }
}

impl<TBuildHasher> AllocatorState<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    #[inline]
    fn return_instance(&self, mut metrics: Metrics<TBuildHasher>) {
        let slot = self
            .cache_clock
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.metrics_cache.len();
        let mut cache = self.metrics_cache[slot].lock().expect("local mutex");
        if cache.len() < cache.capacity() {
            metrics.restart();
            cache.push(metrics)
        }
    }
}

pub struct CachedMetrics<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    allocator: Arc<AllocatorState<TBuildHasher>>,
    metrics: Option<Metrics<TBuildHasher>>,
}

impl<TBuildHasher> Drop for CachedMetrics<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    fn drop(&mut self) {
        // SAFETY: The referent is extracted on this line and the manuallydropped is dropped without further use
        let metrics: Metrics<TBuildHasher> =
            take(&mut self.metrics).expect("drop happens only once");
        self.allocator.return_instance(metrics);
    }
}

impl<TBuildHasher: BuildHasher + Default + Send> Deref for CachedMetrics<TBuildHasher> {
    type Target = Metrics<TBuildHasher>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.metrics.as_ref().expect("cannot access after drop")
    }
}

impl<TBuildHasher: BuildHasher + Default + Send> DerefMut for CachedMetrics<TBuildHasher> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.metrics.as_mut().expect("cannot access after drop")
    }
}

impl<TBuildHasher: BuildHasher + Default + Send> AsMut<Metrics<TBuildHasher>>
    for CachedMetrics<TBuildHasher>
{
    fn as_mut(&mut self) -> &mut Metrics<TBuildHasher> {
        self.metrics.as_mut().expect("cannot access after drop")
    }
}

impl<TBuildHasher: BuildHasher + Default + Send> AsRef<Metrics<TBuildHasher>>
    for CachedMetrics<TBuildHasher>
{
    fn as_ref(&self) -> &Metrics<TBuildHasher> {
        self.metrics.as_ref().expect("cannot access after drop")
    }
}
