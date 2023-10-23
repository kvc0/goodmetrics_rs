use criterion::criterion_main;

mod benchmarks;

criterion_main! {
    benchmarks::aggregation::benches,
    benchmarks::dimensions::benches,
    benchmarks::gauges::benches,
}
