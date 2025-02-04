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

use super::{Hasher, MetricsAllocator};

/// A metrics allocator that prioritizes reusing Metrics objects, but with a gentler
/// Borrow Checker constraint. References are owned, so it's easier to pass them
/// around.
#[derive(Clone)]
pub struct ArcAllocator<TBuildHasher: Send = Hasher> {
    state: Arc<AllocatorState<TBuildHasher>>,
}

struct AllocatorState<TBuildHasher: Send> {
    metrics_cache: [Mutex<Vec<Metrics<TBuildHasher>>>; 8],
    cache_clock: AtomicUsize,
}

impl<TBuildHasher> Default for ArcAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    fn default() -> Self {
        Self::new(1024)
    }
}

impl<TBuildHasher> ArcAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + Send + 'static,
{
    /// Create a new ArcAllocator with a size hint
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

impl<TBuildHasher: Send> MetricsAllocator for ArcAllocator<TBuildHasher>
where
    TBuildHasher: BuildHasher + Default + 'static,
{
    type TMetricsRef = CachedMetrics<TBuildHasher>;

    #[inline]
    fn new_metrics(&self, metrics_name: impl Into<Name>) -> CachedMetrics<TBuildHasher> {
        let slot = self
            .state
            .cache_clock
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.state.metrics_cache.len();
        let metrics = match self.state.metrics_cache[slot].try_lock() {
            Ok(mut cache) => cache.pop(),
            Err(_) => None,
        };
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
        if let Ok(mut cache) = self.metrics_cache[slot].try_lock() {
            if cache.len() < cache.capacity() {
                metrics.restart();
                cache.push(metrics)
            }
        }
    }
}

/// A Metrics reference that caches instances via an Arc to the original allocator's state
pub struct CachedMetrics<TBuildHasher = Hasher>
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
