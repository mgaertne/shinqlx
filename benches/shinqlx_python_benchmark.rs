#![feature(custom_test_frameworks)]
#![test_runner(criterion::runner)]
#![allow(missing_docs, reason = "not part of the public API")]

use core::hint::black_box;

use criterion::Criterion;
use criterion_macro::criterion;
use shinqlx::bench_support;

const STATS_MESSAGE: &str = r#"{"DATA":{"AIRBORNE":false,"AMMO":100,"ARMOR":50,"BOT":false,"BOT_SKILL":null,"HEALTH":125,"HOLDABLE":null,"MATCH_GUID":"8d2b5e0f-0000-0000-0000-000000000000","NAME":"Player","POSITION":{"X":1.0,"Y":2.0,"Z":3.0},"POWERUPS":null,"SPEED":0.0,"VIEW":{"X":0.0,"Y":0.0,"Z":0.0},"WARMUP":false,"WEAPON":"ROCKET"},"TYPE":"PLAYER_STATS"}"#;

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

#[criterion]
fn constructor_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("constructor");
    group.bench_function("powerups", |b| b.iter(bench_support::create_powerups));
    group.bench_function("weapons", |b| b.iter(bench_support::create_weapons));
    group.finish();
}

#[criterion]
fn handle_set_configstring_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    c.bench_function("handle_set_configstring::unobserved_index", |b| {
        b.iter(|| bench_support::handle_set_configstring(black_box(700), black_box("some value")))
    });
}

#[criterion]
fn handle_event_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("handle_event");
    group.bench_function("client_command_scores", |b| {
        b.iter(|| bench_support::handle_client_command(black_box(3), black_box("scores")))
    });
    group.bench_function("server_command_cs", |b| {
        b.iter(|| bench_support::handle_server_command(black_box(3), black_box("cs 700 \"value\"")))
    });
    group.bench_function("stats_message", |b| {
        b.iter(|| bench_support::handle_stats_message(black_box(STATS_MESSAGE)))
    });
    group.bench_function("damage", |b| {
        b.iter(|| bench_support::handle_damage(black_box(3), black_box(Some(4)), black_box(25)))
    });
    group.finish();
}

#[criterion]
fn python_hook_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();
    let handler = bench_support::ClanLikeHandler::default();

    c.bench_function("python_hook::clan_like_set_configstring_handler", |b| {
        b.iter(|| handler.call(black_box(700), black_box("some value")))
    });
}

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
