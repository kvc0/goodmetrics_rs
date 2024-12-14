use std::sync::atomic::AtomicU64;

use crate::{Dimension, Name};

/// A thing that is measured
///
/// The Stat trait may be one thing, or it may be a collection
pub trait Stat {
    /// Record the contents of the stat. Reset any data that needs to be reset, and get the rest on its way to its destination.
    fn record(&self, collector: &mut impl Collector);
}

/// A sink for stats
pub trait Collector {
    /// Collect a subcomponent of the stat at a more specific dimension position - a substat of the current stat.
    fn collect_subcomponent(
        &mut self,
        stat: &impl Stat,
        dimensions: impl IntoIterator<Item = (Name, Dimension)>,
    );
    /// Collect a subcomponent of the stat at an exact dimension position - a root stat, with only the dimensions here.
    fn collect_root_component(
        &mut self,
        stat: &impl Stat,
        dimensions: impl IntoIterator<Item = (Name, Dimension)>,
    );
    /// Collect a number per reporting period
    fn collect_sum(&mut self, name: Name, stat: &impl ResettingNumber);
    /// Inspect a number value that monotonically increases
    fn observe_monotonic(&mut self, name: Name, stat: &impl ObservedNumber);
    /// Inspect a number value that can increase and decrease
    fn observe_number(&mut self, name: Name, stat: &impl ObservedNumber);
    /// Collect and reset a histogram
    fn collect_histogram(
        &mut self,
        name: Name,
        stat: &exponential_histogram::SharedExponentialHistogram,
    );
}

/// Provider strategy for resetting sum stats
pub trait ResettingNumber {
    /// Provider strategy for resetting sum stats
    fn sum_and_reset(&self) -> u64;
}

/// Provider strategy for monotonic and raw number stats
pub trait ObservedNumber {
    /// Provider strategy for monotonic and raw number stats
    fn observe_number(&self) -> u64;
}

impl<T, U> Stat for T
where
    T: core::ops::Deref<Target = U>,
    U: Stat + ?Sized,
{
    fn record(&self, collector: &mut impl Collector) {
        self.deref().record(collector);
    }
}

impl ResettingNumber for AtomicU64 {
    fn sum_and_reset(&self) -> u64 {
        self.swap(0, std::sync::atomic::Ordering::Relaxed)
    }
}

impl ObservedNumber for AtomicU64 {
    fn observe_number(&self) -> u64 {
        self.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod test {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    };

    use crate::{self as goodmetrics, stats::Stat, Dimension, Name};
    use goodmetrics_derive::Stat;

    use super::Collector;

    #[derive(Debug, Default, Stat)]
    pub struct AppStats {
        /// The total number of calls since the application started.
        #[stat(monotonic)]
        total_calls: AtomicU64,

        /// The number of calls per reporting period.
        #[stat(sum)]
        calls: AtomicU64,

        /// The number of active calls - it should be recorded as whatever the value
        /// is. It's a "gauge"
        #[stat(number)]
        active_calls: AtomicU64,

        #[stat(histogram)]
        latency: exponential_histogram::SharedExponentialHistogram,

        /// A subcomponent with stats that could be cloned and reported wherever.
        #[stat(
            component(
                root,
                dimensions = {
                    name="a_subcomponent",
                    foo=3,
                    is_it_real=true,
                }
            )
        )]
        a_subcomponent: Arc<AppSubComponentStats>,

        #[stat(
            component(
                subcomponent,
                dimensions = {
                    name="b_subcomponent",
                }
            )
        )]
        b_subcomponent: Arc<AppSubComponentStats>,
    }

    #[derive(Debug, Default, Stat)]
    pub struct AppSubComponentStats {
        /// An example of subcomponent stats
        #[stat(sum)]
        calls: AtomicU64,
    }

    impl AppStats {
        /// Get a shareable handle to subcomponent stats, for handy referential access.
        pub fn get_a_subcomponent_stats(&self) -> Arc<AppSubComponentStats> {
            self.a_subcomponent.clone()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Collect {
        Subcomponent(Vec<(crate::Name, crate::Dimension)>),
        RootComponent(Vec<(crate::Name, crate::Dimension)>),
        Sum {
            name: crate::Name,
            value: u64,
        },
        Monotonic {
            name: crate::Name,
            value: u64,
        },
        Number {
            name: crate::Name,
            value: u64,
        },
        Histogram {
            name: crate::Name,
            value: exponential_histogram::ExponentialHistogram,
        },
    }

    #[derive(Debug)]
    struct TestCollector {
        actions: mpsc::Sender<Collect>,
    }
    impl Collector for TestCollector {
        fn collect_root_component(
            &mut self,
            stat: &impl super::Stat,
            dimensions: impl IntoIterator<Item = (crate::Name, crate::Dimension)>,
        ) {
            self.actions
                .send(Collect::RootComponent(
                    dimensions.into_iter().collect(),
                ))
                .expect("send must succeed");
            stat.record(self);
        }

        fn collect_subcomponent(
            &mut self,
            stat: &impl super::Stat,
            dimensions: impl IntoIterator<Item = (crate::Name, crate::Dimension)>,
        ) {
            self.actions
                .send(Collect::Subcomponent(
                    dimensions.into_iter().collect(),
                ))
                .expect("send must succeed");
            stat.record(self);
        }

        fn collect_sum(&mut self, name: Name, stat: &impl super::ResettingNumber) {
            self.actions
                .send(Collect::Sum {
                    name,
                    value: stat.sum_and_reset(),
                })
                .expect("send must succeed");
        }

        fn collect_histogram(
            &mut self,
            name: Name,
            stat: &exponential_histogram::SharedExponentialHistogram,
        ) {
            self.actions
                .send(Collect::Histogram {
                    name,
                    value: stat.snapshot_and_reset(),
                })
                .expect("send must succeed");
        }

        fn observe_monotonic(&mut self, name: Name, stat: &impl super::ObservedNumber) {
            self.actions
                .send(Collect::Monotonic {
                    name,
                    value: stat.observe_number(),
                })
                .expect("send must succeed");
        }

        fn observe_number(&mut self, name: Name, stat: &impl super::ObservedNumber) {
            self.actions
                .send(Collect::Number {
                    name,
                    value: stat.observe_number(),
                })
                .expect("send must succeed");
        }
    }

    #[test]
    fn test_app_stats() {
        let app_stats = AppStats::default();
        let subcomponent_stats = app_stats.get_a_subcomponent_stats();

        // Use the generated functions for setting stat values
        app_stats.increase_sum_calls(1);
        app_stats.increase_monotonic_total_calls(2);
        app_stats.set_number_active_calls(3);
        app_stats.accumulate_histogram_latency(5.0);
        subcomponent_stats.increase_sum_calls(4);

        // Check the raw stat values
        assert_eq!(1, app_stats.calls.load(Ordering::Relaxed));
        assert_eq!(2, app_stats.total_calls.load(Ordering::Relaxed));
        assert_eq!(3, app_stats.active_calls.load(Ordering::Relaxed));
        assert_eq!(4, subcomponent_stats.calls.load(Ordering::Relaxed));

        // Set up a fake collector to record the stats.
        let (tx, rx) = mpsc::channel();
        let mut collector = TestCollector { actions: tx };
        app_stats.record(&mut collector);

        // the histogram should be the same as a default histogram that has accumulated 5.0
        let mut expected_histogram = exponential_histogram::ExponentialHistogram::default();
        expected_histogram.accumulate(5.0);
        // All names and values from the derive macro are static, so we use Str instead of String or Shared.
        assert_eq!(
            vec![
                Collect::Monotonic {
                    name: Name::Str("total_calls"),
                    value: 2
                },
                Collect::Sum {
                    name: Name::Str("calls"),
                    value: 1
                },
                Collect::Number {
                    name: Name::Str("active_calls"),
                    value: 3
                },
                Collect::Histogram {
                    name: Name::Str("latency"),
                    value: expected_histogram
                },
                Collect::RootComponent(vec![
                    (Name::Str("name"), Dimension::Str("a_subcomponent")),
                    (Name::Str("foo"), Dimension::Number(3)),
                    (Name::Str("is_it_real"), Dimension::Boolean(true)),
                ]),
                Collect::Sum {
                    name: Name::Str("calls"),
                    value: 4
                },
                Collect::Subcomponent(vec![(
                    Name::Str("name"),
                    Dimension::Str("b_subcomponent")
                ),]),
                Collect::Sum {
                    name: Name::Str("calls"),
                    value: 0
                },
            ],
            rx.try_iter().collect::<Vec<_>>()
        );
    }
}
