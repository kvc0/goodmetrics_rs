use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
    time::Duration,
};

use criterion::Criterion;
use hyper::{header::HeaderName, http::HeaderValue};

use goodmetrics::{
    allocator::pooled_metrics_allocator::PooledMetricsAllocator,
    downstream::{
        channel_connection::get_channel,
        goodmetrics_downstream::{create_preaggregated_goodmetrics_batch, GoodmetricsDownstream},
    },
    metrics_factory::{MetricsFactory, RecordingScope},
    pipeline::aggregating_sink::AggregatingSink,
};
use tokio::task::LocalSet;

pub fn goodmetrics_demo(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();
    let auth = option_env!("GOODMETRICS_AUTH").unwrap_or("none");
    let endpoint = option_env!("GOODMETRICS_SERVER").expect(
        "You need to provide a GOODMETRICS_SERVER=https://1.2.3.4:5678 environment variable",
    );

    // Set up the bridge between application metrics threads and the metrics downstream thread
    let sink = Arc::new(AggregatingSink::new());
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
                endpoint,
                || None,
                Some((
                    HeaderName::from_static("authorization"),
                    HeaderValue::try_from(auth).expect("invalid authorization value"),
                )),
            )
            .await
            .expect("i can make a channel to goodmetrics");
            let mut downstream = GoodmetricsDownstream::new(
                channel,
                HashMap::from_iter(vec![("application".to_string(), "bench".to_string())]),
            );

            let metrics_tasks = LocalSet::new();

            metrics_tasks.spawn_local(async move {
                task_sink
                    .drain_into_sender_forever(
                        Duration::from_secs(1),
                        sender,
                        create_preaggregated_goodmetrics_batch,
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
    criterion.bench_function("goodmetrics", |bencher| {
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
