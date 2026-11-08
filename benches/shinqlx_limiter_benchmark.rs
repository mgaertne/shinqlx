#![feature(custom_test_frameworks)]
#![test_runner(criterion::runner)]
#![allow(missing_docs, reason = "not part of the public API")]

use core::hint::black_box;

use criterion::Criterion;
use criterion_macro::criterion;
use shinqlx::bench_support::QueryLimiterCase;

#[criterion]
fn query_limiter_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_limiter");

    let mut unlimited = QueryLimiterCase::with_limits(0.0, 0.0, 0.0);
    group.bench_function("disabled", |b| {
        b.iter(|| unlimited.allow(black_box([203, 0, 113, 7]), black_box(1_000)))
    });

    let mut generous = QueryLimiterCase::with_limits(1.0e9, 1.0e9, 0.0);
    group.bench_function("known_source_allowed", |b| {
        b.iter(|| generous.allow(black_box([203, 0, 113, 7]), black_box(1_000)))
    });

    let mut exhausted = QueryLimiterCase::with_limits(0.0001, 1.0, 0.0);
    exhausted.allow([203, 0, 113, 7], 1_000);
    group.bench_function("known_source_dropped", |b| {
        b.iter(|| exhausted.allow(black_box([203, 0, 113, 7]), black_box(1_000)))
    });

    let mut globally_limited = QueryLimiterCase::with_limits(0.0, 0.0, 0.0001);
    globally_limited.allow([203, 0, 113, 7], 1_000);
    group.bench_function("global_limit_dropped", |b| {
        b.iter(|| globally_limited.allow(black_box([203, 0, 113, 7]), black_box(1_000)))
    });

    let mut defaults = QueryLimiterCase::with_defaults();
    let mut counter = 0_u32;
    group.bench_function("many_sources_default_limits", |b| {
        b.iter(|| {
            counter = counter.wrapping_add(1);
            let source = (counter % 4096).to_be_bytes();
            defaults.allow(black_box(source), black_box(u64::from(counter) / 4096))
        })
    });
    group.finish();
}
