use std::{sync::{Arc, mpsc}, time::Duration};

use criterion::{criterion_group, criterion_main, Criterion};
use hyper::{header::HeaderName, http::HeaderValue};

use goodmetrics::{pipeline::aggregating_sink::AggregatingSink, downstream::{opentelemetry_downstream::{OpenTelemetryDownstream, create_preaggregated_opentelemetry_batch}, channel_connection::get_channel}, metrics_factory::{MetricsFactory, RecordingScope}, allocator::pooled_metrics_allocator::PooledMetricsAllocator};
use tokio::task::LocalSet;

fn lightstep_demo(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();

    // Set up the bridge between application metrics threads and the metrics downstream thread
    let sink = Arc::new(AggregatingSink::new());
    let (sender, receiver) = mpsc::sync_channel(128);

    // Configure downstream metrics thread tasks
    let task_sink = sink.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("should be able to make tokio runtime");
        runtime.block_on(async move {
            let channel = get_channel(
                "https://ingest.lightstep.com",
                true,
                Some((
                    HeaderName::from_static("lightstep-access-token"),
                    HeaderValue::from_static(option_env!("LIGHTSTEP_ACCESS_TOKEN").expect("you need to provide LIGHTSTEP_ACCESS_TOKEN")),
                )),
            ).await.expect("i can make a channel to lightstep");
            let mut downstream = OpenTelemetryDownstream::new(channel);
    
            let metrics_tasks = LocalSet::new();

            metrics_tasks.spawn_local(async move {
                task_sink
                    .drain_into_sender_forever(
                        Duration::from_secs(1),
                        sender,
                        create_preaggregated_opentelemetry_batch,
                    ).await;
            });
            metrics_tasks.spawn_local(async move {
                downstream.send_batches_forever(receiver).await;
            });

            metrics_tasks.await;
        });
    });

    // Prepare the application metrics (we only need the sink to make a factory - you can have a factory per thread if you want)
    let metrics_factory: MetricsFactory<PooledMetricsAllocator, Arc<AggregatingSink>> = MetricsFactory::new(sink);

    // Finally, run the application and record metrics
    criterion.bench_function("demo", |bencher| {
        bencher.iter(|| {
            let metrics = metrics_factory.record_scope("demo");
            let _scope = metrics.time("timed_delay");
        });
    });
}

criterion_group!(benches, lightstep_demo);
criterion_main!(benches);
