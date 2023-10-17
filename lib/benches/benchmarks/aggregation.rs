use std::{
    cmp::{max, min},
    sync::mpsc::sync_channel,
    time::{Duration, Instant},
};

use criterion::Criterion;
use goodmetrics::{
    allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator,
    downstream::opentelemetry_downstream::create_preaggregated_opentelemetry_batch,
    metrics_factory::{MetricsFactory, RecordingScope},
    pipeline::{
        aggregator::{Aggregator, DistributionMode},
        stream_sink::StreamSink,
    },
};

pub fn aggregation(criterion: &mut Criterion) {
    // env_logger::builder().is_test(false).try_init().unwrap();

    let mut group = criterion.benchmark_group("aggregation");
    group.throughput(criterion::Throughput::Elements(1));

    let (sink, receiver) = StreamSink::new();
    let aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<_>> =
        MetricsFactory::new(sink);
    let (batch_sender, _r) = sync_channel(128);
    aggregator
        .spawn_aggregation_thread(
            Duration::from_secs(1),
            batch_sender,
            create_preaggregated_opentelemetry_batch,
        )
        .expect("it should spawn");

    for threads in [1, 2, 4, 8, 16] {
        group.bench_function(format!("concurrency-{threads:02}"), |bencher| {
            bencher.iter_custom(|iterations| {
                let thread_count = max(1, min(threads, iterations));
                let iterations_per_thread = iterations / thread_count;

                let start = Instant::now();
                std::thread::scope(|scope| {
                    for _ in 0..thread_count {
                        scope.spawn(|| {
                            for i in 0..iterations_per_thread {
                                let metrics = metrics_factory.record_scope("demo");
                                let _scope = metrics.time("timed_delay");
                                metrics.measurement("ran", 1);
                                metrics.dimension("mod", i % 8);
                            }
                        });
                    }
                });

                start.elapsed()
            });
        });
    }
}

criterion::criterion_group!(benches, aggregation);
