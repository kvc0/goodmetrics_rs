# <img src="https://user-images.githubusercontent.com/3454741/151748581-1ad6c34c-f583-4813-b878-d19c98ec3427.png" width="108em" align="center"/> Goodmetrics: Rust



# About
This is the Rust goodmetrics client. It bundles an opentelemetry protocol downstream and
some performance tools like the PooledMetricsAllocator.
To use any grpc downstream (goodmetrics or opentelemetry) you will need a tokio runtime.

# How to use
See [the lightstep demo](./src/benches/lightstep.rs) for a complete setup and usage example with opentelemetry.

Once you have a configured MetricsFactory, the way you use Metrics does not change with subsequent
updates to the configured downstream(s):
```rust
let metrics = metrics_factory.record_scope("demo"); // By default, includes a "demo_totaltime" histogram measurement
let _scope = metrics.time("timed_delay"); // you can time additional scopes
metrics.measurement("ran", 1); // measurements can be plain numbers; when preaggregated they are StatisticSets (min/max/sum/count)
metrics.dimension("mod", i % 8); // you can add dimensions to a Metrics whenever you want. All measurements in this Metrics record are dimensioned by this value.
metrics.distribution("some_continuous_value", instantaneous_network_bandwidth); // histograms are aggregated sparsely, and truncated to 2 significant figures (base 10).
```

# Development
Use `cargo ws version minor` to update version.
