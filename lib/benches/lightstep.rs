use std::collections::BTreeMap;
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

use criterion::Criterion;
use hyper::{header::HeaderName, http::HeaderValue};

use goodmetrics::types::{Dimension, Name};
use goodmetrics::{
    allocator::pooled_metrics_allocator::PooledMetricsAllocator,
    downstream::{
        channel_connection::get_channel,
        opentelemetry_downstream::{
            create_preaggregated_opentelemetry_batch, OpenTelemetryDownstream,
        },
    },
    metrics_factory::{MetricsFactory, RecordingScope},
    pipeline::aggregating_sink::{AggregatingSink, DistributionMode},
};
use tokio::task::LocalSet;
use tokio_rustls::rustls::{OwnedTrustAnchor, RootCertStore};

pub fn lightstep_demo(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();

    // Set up the bridge between application metrics threads and the metrics downstream thread
    let sink = Arc::new(AggregatingSink::new(DistributionMode::Histogram));
    let (sender, receiver) = mpsc::sync_channel(128);

    // Configure downstream metrics thread tasks
    let task_sink = sink.clone();
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
                    store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
                        |trust_anchor| {
                            OwnedTrustAnchor::from_subject_spki_name_constraints(
                                trust_anchor.subject,
                                trust_anchor.spki,
                                trust_anchor.name_constraints,
                            )
                        },
                    ));
                    Some(store)
                },
                Some((
                    HeaderName::from_static("lightstep-access-token"),
                    HeaderValue::from_static(
                        option_env!("LIGHTSTEP_ACCESS_TOKEN")
                            .expect("you need to provide LIGHTSTEP_ACCESS_TOKEN"),
                    ),
                )),
            )
            .await
            .expect("i can make a channel to lightstep");
            let mut downstream = OpenTelemetryDownstream::new(
                channel,
                Some(BTreeMap::from_iter(vec![(
                    Name::from("name"),
                    Dimension::from("value i guess"),
                )])),
            );

            let metrics_tasks = LocalSet::new();

            metrics_tasks.spawn_local(async move {
                task_sink
                    .drain_into_sender_forever(
                        Duration::from_secs(1),
                        sender,
                        create_preaggregated_opentelemetry_batch,
                    )
                    .await;
            });
            metrics_tasks.spawn_local(async move {
                downstream.send_batches_forever(receiver).await;
            });

            metrics_tasks.await;
        });
    });

    // Prepare the application metrics (we only need the sink to make a factory - you can have a factory per thread if you want)
    let metrics_factory: MetricsFactory<PooledMetricsAllocator, Arc<AggregatingSink>> =
        MetricsFactory::new(sink);

    // Finally, run the application and record metrics
    criterion.bench_function("lightstep", |bencher| {
        let mut i = 0_u64;
        bencher.iter(|| {
            i += 1;

            let metrics = metrics_factory.record_scope("demo");
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
