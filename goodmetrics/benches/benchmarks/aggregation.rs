use std::{
    cmp::{max, min},
    time::{Duration, Instant},
};

use criterion::{measurement::WallTime, BenchmarkGroup, Criterion};
use goodmetrics::{
    allocator::AlwaysNewMetricsAllocator,
    downstream::OpentelemetryBatcher,
    pipeline::{Aggregator, DistributionMode, StreamSink},
    MetricsFactory,
};
use tokio::sync::mpsc;

pub fn aggregation(criterion: &mut Criterion) {
    // env_logger::builder().is_test(false).try_init().unwrap();

    let mut group = criterion.benchmark_group("aggregation");
    group.throughput(criterion::Throughput::Elements(1));
    bench_distribution_mode(&mut group, DistributionMode::Histogram);
    bench_distribution_mode(
        &mut group,
        DistributionMode::ExponentialHistogram {
            max_buckets: 160,
            desired_scale: 2,
        },
    );
    bench_distribution_mode(&mut group, DistributionMode::TDigest);
}

fn bench_distribution_mode(
    group: &mut BenchmarkGroup<'_, WallTime>,
    distribution_mode: DistributionMode,
) {
    let (sink, receiver) = StreamSink::new();
    let aggregator = Aggregator::new(receiver, distribution_mode);
    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<_>> =
        MetricsFactory::new(sink);
    let (aggregated_batch_sender, _r) = mpsc::channel(128);
    let metrics_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(1)
        .build()
        .expect("I can make a runtime");
    metrics_runtime.spawn(aggregator.aggregate_metrics_forever(
        Duration::from_secs(1),
        aggregated_batch_sender,
        OpentelemetryBatcher,
    ));

    for threads in [1, 4, 16] {
        group.bench_function(
            format!("{distribution_mode}-concurrency-{threads:02}"),
            |bencher| {
                bencher.iter_custom(|iterations| {
                    let thread_count = max(1, min(threads, iterations));
                    let iterations_per_thread = iterations / thread_count;

                    let start = Instant::now();
                    std::thread::scope(|scope| {
                        for _ in 0..thread_count {
                            scope.spawn(|| {
                                for i in 0..iterations_per_thread {
                                    let mut metrics = metrics_factory.record_scope("demo");
                                    let _scope = metrics.time("timed_delay");
                                    metrics.measurement("ran", 1);
                                    metrics.dimension("mod", i % 8);
                                }
                            });
                        }
                    });

                    start.elapsed()
                });
            },
        );
    }
}

criterion::criterion_group!(benches, aggregation);
