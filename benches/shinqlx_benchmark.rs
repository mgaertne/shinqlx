#![feature(custom_test_frameworks)]
#![test_runner(criterion::runner)]
#![allow(missing_docs)]

use core::hint::black_box;

use criterion::Criterion;
use criterion_macro::criterion;
#[cfg(feature = "bench-support")]
use shinqlx::bench_support;
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

#[cfg(feature = "bench-support")]
#[criterion]
fn clean_text_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("clean_text");
    for (name, text) in [
        ("plain", "PlayerName"),
        ("colored", "^1Red^7Player^4Name"),
        (
            "long_colored",
            "^1This ^2is ^3a ^4rather ^5long ^6chat ^7message ^1with ^2many ^3color ^4tags ^5in ^6it",
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| bench_support::clean_text(black_box(text)))
        });
    }
    group.finish();
}

#[cfg(feature = "bench-support")]
#[criterion]
fn constructor_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("constructor");
    group.bench_function("powerups", |b| b.iter(bench_support::create_powerups));
    group.bench_function("weapons", |b| b.iter(bench_support::create_weapons));
    group.finish();
}

#[cfg(feature = "bench-support")]
#[criterion]
fn handle_frame_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("handle_frame");
    group.bench_function("idle", |b| b.iter(bench_support::handle_frame));
    group.bench_function("with_three_tasks", |b| {
        b.iter(|| {
            bench_support::queue_next_frame_tasks(black_box(3));
            bench_support::handle_frame();
        })
    });
    group.finish();
}
