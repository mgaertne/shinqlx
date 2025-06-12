#![feature(c_variadic, cold_path, custom_test_frameworks)]
#![test_runner(criterion::runner)]
#![allow(missing_docs)]

use core::hint::black_box;

use criterion::Criterion;
use criterion_macro::criterion;
use shinqlx::quake_live_functions::QuakeLiveFunction;

fn original_test_func() -> String {
    "original".into()
}

fn replacement_test_func() -> String {
    "replacement".into()
}

#[criterion]
fn create_and_enable_generic_detour_benchmark(c: &mut Criterion) {
    c.bench_function(
        "quake_live_function::create_and_enable_generic_detour",
        |b| {
            b.iter(|| {
                let _ = QuakeLiveFunction::Com_Printf.create_and_enable_generic_detour(
                    black_box(original_test_func as fn() -> String),
                    black_box(replacement_test_func as fn() -> String),
                );
            })
        },
    );
}
