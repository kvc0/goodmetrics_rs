# <img src="https://user-images.githubusercontent.com/3454741/151748581-1ad6c34c-f583-4813-b878-d19c98ec3427.png" width="108em" align="center"/> Goodmetrics: Rust

## About

This is the Rust goodmetrics client. It bundles an opentelemetry protocol downstream and
some performance tools like the PooledMetricsAllocator.
To use any grpc downstream (goodmetrics or opentelemetry) you will need a tokio runtime.

## How to use

See [the lightstep demo](./src/benches/lightstep.rs) for a complete setup and usage example with opentelemetry.

### Simple example

Once you have a configured MetricsFactory, the way you use Metrics does not change with subsequent
updates to the configured downstream(s):

```rust
let mut metrics = metrics_factory.record_scope("demo"); // By default, includes a "demo_totaltime" histogram measurement, capturing the time it took to complete the unit of work
{
    let _scope = metrics.time("timed_delay"); // you can time additional scopes
    my_timed_delay();
}
metrics.measurement("ran", 1); // measurements can be plain numbers; when preaggregated they are StatisticSets (min/max/sum/count)
metrics.sum("runs_count", 1); // If you do not need a StatisticSet but rather a simple counter,
                              // sum will produce a simple Gauge
metrics.dimension("mod", i % 8); // you can add dimensions to a Metrics whenever you want. All measurements in this Metrics record are dimensioned by this value.
metrics.distribution("some_continuous_value", instantaneous_network_bandwidth); // histograms are aggregated sparsely, and truncated to 2 significant figures (base 10).
```

Once the `metrics` object is dropped from scope, it will export metrics to your desired ingest. If you need to publish metrics at some immediate point, you can manually `drop()` the object.

### Record scope without measuring time

Not every unit of work needs a measurement of how long it took to complete. Simply create a metrics object by calling `record_scope_with_behavior` and passing in your desired behavior.

For example:

```rust
let mut metrics = metrics_factory.record_scope_with_behavior(
    "demo",
    MetricsBehavior::SuppressTotalTime,
);
```

### Create a dimension with a default value

In a distributed system, a unit of work may not fully complete and the `metrics` object will fall out of scope, causing any existing dimensions or measurements to be exported. This can lead to confusing behavior if a dimension was expected, but not present.

Using a `guarded_dimension` can provide a default value and export it should the object be dropped before expected:

```rust
let mut metrics = metrics_factory.record_scope("demo");
let result_dimension = metrics.guarded_dimension("my_api_result", "dropped_early");

// Perform work...
let result = serve_api_request(request);

match result {
    Ok(_) => {
        // Explicitly set the guarded dimension and overwrite the default value
        result_dimension.set("ok")
    },
    Err(_e) => {
        // Explicitly set the guarded dimension and overwrite the default value
        result_dimension.set("error");
        metrics.sum("errors", 1_u64)
    }
}
```

### Available metrics functions and when to use them

Latest docs can always be found on [docs.rs](https://docs.rs/goodmetrics/latest/goodmetrics/).

A basic knowledge of [OpenTelemetry metric types](https://opentelemetry.io/docs/specs/otel/metrics/data-model/#metric-points) is expected, but a shortened guide can be summarized below:

#### `metrics.measurement("name", value)`

* Use when you only need `min`, `max`, `sum`, and `count` of data points received.
* Use `sum()` instead if you only need `sum` or `count`.
* Use `distribution()` instead if you need percentiles of values.
* Aggregates locally into a `StatisticsSet`

#### `metrics.distribution("name", value)`

* Useful for identifying percentiles of data, like the 99th percentile of request sizes.
* Aggregates locally into a `Histogram` or `ExponentialHistogram` depending on configuration.

#### `metrics.sum("name", value)`

Adds up all the sum calls for "name" within each reporting period.

* Useful for tracking a counter such as bytes stored on disk, or number of tokens consumed.
* Aggregates locally into a `Sum`.

#### `let _guard = metrics.time("name")`

* For measuring how long something takes within the given scope of code.
* Aggregates locally into `Histogram` or `ExponentialHistogram`, depending on configuration.
* Unit for timers is nanoseconds.

## Development

Use `cargo ws version minor` to update version.
