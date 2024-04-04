use std::{
    cmp::{max, min},
    time::Instant,
};

use criterion::Criterion;
use goodmetrics::{
    allocator::AlwaysNewMetricsAllocator, metrics::Metrics, metrics_factory::MetricsFactory,
    pipeline::Sink,
};

struct DropSink;
impl Sink<Metrics> for DropSink {
    fn accept(&self, _: Metrics) {}
}

pub fn gauges(criterion: &mut Criterion) {
    // env_logger::builder().is_test(false).try_init().unwrap();

    let mut group = criterion.benchmark_group("gauges");
    group.throughput(criterion::Throughput::Elements(1));

    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, DropSink> =
        MetricsFactory::new(DropSink);

    let cached_gauge = metrics_factory.gauge("contention", "is okay");

    for threads in [1, 2, 4, 8, 16] {
        group.bench_function(format!("concurrency-{threads:02}"), |bencher| {
            bencher.iter_custom(|iterations| {
                let thread_count = max(1, min(threads, iterations));
                let iterations_per_thread: i64 = (iterations / thread_count) as i64;

                let start = Instant::now();
                std::thread::scope(|scope| {
                    for _ in 0..thread_count {
                        scope.spawn(|| {
                            for i in 0..iterations_per_thread {
                                cached_gauge.observe(i % 8);
                            }
                        });
                    }
                });

                start.elapsed()
            });
        });
    }
}

criterion::criterion_group!(benches, gauges);
