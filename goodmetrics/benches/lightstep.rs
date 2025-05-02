use std::time::Duration;

use criterion::Criterion;
use goodmetrics::allocator::AlwaysNewMetricsAllocator;
use goodmetrics::pipeline::StreamSink;

use goodmetrics::{
    downstream::{get_client, OpenTelemetryDownstream, OpentelemetryBatcher},
    pipeline::{Aggregator, DistributionMode},
    MetricsFactory,
};
use tokio::join;
use tokio::sync::mpsc;
use tokio_rustls::rustls::RootCertStore;

#[allow(clippy::unwrap_used)]
pub fn lightstep_demo(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();

    // Set up the bridge between application metrics threads and the metrics downstream thread
    let (sink, receiver) = StreamSink::new();
    let aggregator = Aggregator::new(receiver, DistributionMode::Histogram);
    let (aggregated_batch_sender, receiver) = mpsc::channel(128);

    // Configure downstream metrics thread tasks
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should be able to make tokio runtime");
        runtime.block_on(async move {
            let channel = get_client(
                "https://ingest.lightstep.com",
                || {
                    let store = RootCertStore {
                        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
                    };
                    Some(store)
                },
                goodmetrics::proto::opentelemetry::collector::metrics::v1::metrics_service_client::MetricsServiceClient::with_origin,
            )
            .expect("i can make a channel to lightstep");
            let downstream = OpenTelemetryDownstream::new_with_dimensions(
                channel,
                Some((
                    "lightstep-access-token",
                    std::env::var("LIGHTSTEP_ACCESS_TOKEN")
                        .expect("you need to provide LIGHTSTEP_ACCESS_TOKEN")
                        .parse()
                        .expect("must be headerizable")
                )),
                [(
                    "name",
                    "value i guess",
                )],
            );

            let _ = join!(
                aggregator.aggregate_metrics_forever(
                    Duration::from_secs(1),
                    aggregated_batch_sender,
                    OpentelemetryBatcher,
                ),
                downstream.send_batches_forever(receiver),
            );
        });
    });

    // Prepare the application metrics (we only need the sink to make a factory - you can have a factory per thread if you want)
    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<_>> =
        MetricsFactory::new(sink);

    // Finally, run the application and record metrics
    let mut group = criterion.benchmark_group("lightstep");
    group.throughput(criterion::Throughput::Elements(1));
    group.bench_function("demo", |bencher| {
        let mut i = 0_u64;
        bencher.iter(|| {
            i += 1;

            let mut metrics = metrics_factory.record_scope("demo");
            let _scope = metrics.time("timed_delay");
            metrics.measurement("ran", 1);
            metrics.dimension("mod", i % 8);
        });
    });
}

criterion::criterion_group!(benches, lightstep_demo);
criterion::criterion_main! {
    benches,
}
