use std::collections::BTreeMap;
use std::time::Duration;

use criterion::Criterion;
use goodmetrics::allocator::always_new_metrics_allocator::AlwaysNewMetricsAllocator;
use goodmetrics::pipeline::stream_sink::StreamSink;
use hyper::{header::HeaderName, http::HeaderValue};

use goodmetrics::types::{Dimension, Name};
use goodmetrics::{
    downstream::{
        channel_connection::get_channel,
        opentelemetry_downstream::{
            create_preaggregated_opentelemetry_batch, OpenTelemetryDownstream,
        },
    },
    metrics_factory::{MetricsFactory, RecordingScope},
    pipeline::aggregator::{Aggregator, DistributionMode},
};
use tokio::join;
use tokio::sync::mpsc;
use tokio_rustls::rustls::{OwnedTrustAnchor, RootCertStore};

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
            let channel = get_channel(
                "https://ingest.lightstep.com",
                || {
                    let mut store = RootCertStore::empty();
                    store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(
                        |trust_anchor| {
                            OwnedTrustAnchor::from_subject_spki_name_constraints(
                                trust_anchor.subject.to_vec(),
                                trust_anchor.subject_public_key_info.to_vec(),
                                trust_anchor
                                    .name_constraints
                                    .as_ref()
                                    .map(|der| der.to_vec()),
                            )
                        },
                    ));
                    Some(store)
                },
                Some((
                    HeaderName::from_static("lightstep-access-token"),
                    HeaderValue::from_str(
                        &std::env::var("LIGHTSTEP_ACCESS_TOKEN")
                            .expect("you need to provide LIGHTSTEP_ACCESS_TOKEN"),
                    )
                    .expect("access token must be headerizable"),
                )),
            )
            .expect("i can make a channel to lightstep");
            let downstream = OpenTelemetryDownstream::new(
                channel,
                Some(BTreeMap::from_iter(vec![(
                    Name::from("name"),
                    Dimension::from("value i guess"),
                )])),
            );

            let _ = join!(
                aggregator.aggregate_metrics_forever(
                    Duration::from_secs(1),
                    aggregated_batch_sender,
                    create_preaggregated_opentelemetry_batch
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
