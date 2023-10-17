use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
};

use criterion::{black_box, Criterion};
use goodmetrics::types::Dimension;

#[allow(clippy::unwrap_used)]
pub fn dimension_comparison(criterion: &mut Criterion) {
    env_logger::builder().is_test(false).try_init().unwrap();

    let mut group = criterion.benchmark_group("dimensions");

    let s: &'static str = "a static string";
    group.bench_with_input("static string", &s, |bencher, s| {
        bencher.iter(|| black_box(Dimension::Str(s)))
    });

    let s = "an owned string".to_string();
    group.bench_with_input("string", &s, |bencher, s| {
        bencher.iter(|| {
            // really this is benchmarking string copy versus arc increment/decrement
            black_box(Dimension::String(s.clone()))
        })
    });

    let s = Arc::new("a shared string".to_string());
    group.bench_with_input("arc string", &s, |bencher, s| {
        bencher.iter(|| {
            // really this is benchmarking string copy versus arc increment/decrement
            black_box(Dimension::Shared(s.clone()))
        })
    });

    // In case you were thinking about making a pool of arc strings to get the best of both worlds,
    // this strategy could yield you improved String performance. It is on the order of the regular
    // owned String strategy, but the latency clusters at the bottom of the range for String cloning.
    // This would also help control your memory usage though, so it might be worth doing for reasons
    // other than simple latency.
    let s = Arc::new("interned arc string".to_string());
    group.bench_with_input("best-case interned arc", &s, |bencher, s| {
        bencher.iter(|| {
            let mut state = DefaultHasher::new();
            s.hash(&mut state);
            black_box(state.finish());
            black_box(Dimension::Shared(s.clone()))
        })
    });

    // If HashMap is hard to make faster than a string copy. You probably don't want to make a simple
    // string interning solution like this, as it is around 30% slower than just cloning strings.
    let interned: HashMap<String, Arc<String>> = HashMap::from_iter(vec![(
        "interned arc string".to_string(),
        Arc::new("interned arc string".to_string()),
    )]);
    let temporary_request_string = "interned arc string".to_string();
    group.bench_with_input(
        "more-likely interned arc",
        &(interned, temporary_request_string),
        |bencher, (interned, lookup)| {
            bencher.iter(|| black_box(Dimension::Shared(interned[lookup].clone())))
        },
    );
}

criterion::criterion_group!(benches, dimension_comparison);
