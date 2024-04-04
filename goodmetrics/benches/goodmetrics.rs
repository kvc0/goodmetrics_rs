use std::{sync::Arc, time::Duration};

use criterion::Criterion;
use hyper::{header::HeaderName, http::HeaderValue};

use goodmetrics::{
    allocator::AlwaysNewMetricsAllocator,
    downstream::{
        channel_connection::get_channel,
        goodmetrics_downstream::{create_preaggregated_goodmetrics_batch, GoodmetricsDownstream},
    },
    metrics_factory::{MetricsFactory, RecordingScope},
    pipeline::{
        aggregator::{Aggregator, DistributionMode},
        stream_sink::StreamSink,
    },
};
use tokio::{join, sync::mpsc};

#[allow(clippy::unwrap_used)]
pub fn goodmetrics_demo(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();
    let auth = std::env::var("GOODMETRICS_AUTH").unwrap_or("none".to_string());
    let endpoint = std::env::var("GOODMETRICS_SERVER").expect(
        "You need to provide a GOODMETRICS_SERVER=https://1.2.3.4:5678 environment variable",
    );

    // Set up the bridge between application metrics threads and the metrics downstream thread
    let (sink, receiver) = StreamSink::new();
    let aggregator = Aggregator::new(receiver, DistributionMode::TDigest);
    let (aggregated_batch_sender, receiver) = mpsc::channel(128);

    // Configure downstream metrics thread tasks
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should be able to make tokio runtime");
        runtime.block_on(async move {
            let channel = get_channel(
                &endpoint,
                || None,
                Some((
                    HeaderName::from_static("authorization"),
                    HeaderValue::try_from(auth).expect("invalid authorization value"),
                )),
            )
            .expect("i can make a channel to goodmetrics");
            let downstream = GoodmetricsDownstream::new(channel, [("application", "bench")]);

            join!(
                aggregator.aggregate_metrics_forever(
                    Duration::from_secs(1),
                    aggregated_batch_sender,
                    create_preaggregated_goodmetrics_batch
                ),
                downstream.send_batches_forever(receiver),
            );
        });
    });

    // Prepare the application metrics (we only need the sink to make a factory - you can have a factory per thread if you want)
    let metrics_factory: MetricsFactory<AlwaysNewMetricsAllocator, StreamSink<_>> =
        MetricsFactory::new(sink);
    let metrics_factory = Arc::new(metrics_factory);

    // Finally, run the application and record metrics
    criterion.bench_function("goodmetrics", |bencher| {
        let mut i = 0_u64;
        let metrics_factory = metrics_factory.clone();
        bencher.iter(move || {
            i += 1;

            let mut metrics = metrics_factory.record_scope("demo");
            let _scope = metrics.time("timed_delay");
            metrics.measurement("ran", 1);
            metrics.dimension("mod", i % 8);
            std::thread::sleep(Duration::from_micros(1));
        });
    });
}

criterion::criterion_group!(benches, goodmetrics_demo);
criterion::criterion_main! {
    benches,
}
