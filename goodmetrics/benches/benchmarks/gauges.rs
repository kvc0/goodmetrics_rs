use std::{
    cmp::{max, min},
    sync::{Arc, Barrier},
    time::Instant,
};

use criterion::{measurement::WallTime, BenchmarkId, Criterion};
use goodmetrics::{GaugeFactory, StatisticSetHandle, SumHandle};

pub fn gauges(criterion: &mut Criterion) {
    // env_logger::builder().is_test(false).try_init().unwrap();

    let mut group = criterion.benchmark_group("gauges");

    let gauge_factory = GaugeFactory::default();
    let spread = 1000000;

    let cached_gauge = gauge_factory.gauge_statistic_set("contention", "statistic_set");
    bench_gauge(
        "statistic_set",
        spread,
        &mut group,
        cached_gauge,
        StatisticSetHandle::observe,
    );

    let cached_gauge = gauge_factory.dimensioned_gauge_sum("contention", "sum", Default::default());
    bench_gauge("sum", spread, &mut group, cached_gauge, SumHandle::observe);

    let cached_gauge =
        gauge_factory.dimensioned_gauge_histogram("contention", "histogram", Default::default());
    bench_gauge(
        "histogram",
        spread,
        &mut group,
        cached_gauge,
        |gauge, value| gauge.observe(value),
    );
}

fn bench_gauge<T: Send + Clone + 'static>(
    function_name: &str,
    value_spread: i64,
    group: &mut criterion::BenchmarkGroup<'_, WallTime>,
    cached_gauge: T,
    function: fn(&T, i64),
) {
    for threads in [1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16] {
        group.bench_function(BenchmarkId::new(function_name, threads), |bencher| {
            bencher.iter_custom(|iterations| {
                let thread_count = max(1, min(threads, iterations / 10000));
                let iterations_per_thread: i64 = (iterations / thread_count) as i64;

                let barrier = Arc::new(Barrier::new(thread_count as usize + 1));
                for _ in 0..thread_count {
                    let barrier = barrier.clone();
                    let cached_gauge = cached_gauge.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        for i in 0..iterations_per_thread {
                            function(&cached_gauge, i % value_spread);
                        }
                        barrier.wait();
                    });
                }

                barrier.wait();
                let start = Instant::now();
                barrier.wait();
                start.elapsed()
            });
        });
    }
}

criterion::criterion_group!(benches, gauges);
