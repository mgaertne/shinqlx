#![feature(custom_test_frameworks)]
#![test_runner(criterion::runner)]
#![allow(missing_docs, reason = "not part of the public API")]

use core::hint::black_box;

use criterion::{Criterion, Throughput};
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
    group.bench_function("client_command_dispatcher_scores", |b| {
        b.iter(|| bench_support::client_command_dispatcher(black_box(3), black_box("scores")))
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
fn dispatch_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("dispatch");
    for (handlers, arguments) in [(0, 0), (1, 0), (1, 3), (5, 3)] {
        let case = bench_support::DispatchCase::new(handlers, arguments);
        group.bench_function(format!("{handlers}_handlers_{arguments}_arguments"), |b| {
            b.iter(|| case.dispatch())
        });
    }
    group.finish();
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

const ITERATIONS: usize = 1_000;

fn python_case_group(
    c: &mut Criterion,
    group_name: &str,
    setup: &str,
    cases: &[(&str, &str)],
    iterations: usize,
) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(iterations as u64));
    for (name, body) in cases {
        let case = bench_support::PythonCase::new(setup, body);
        group.bench_function(*name, |b| b.iter(|| case.run(black_box(iterations))));
    }
    group.finish();
}

#[criterion]
fn enum_comparison_benchmark(c: &mut Criterion) {
    python_case_group(
        c,
        "enum_comparison",
        "red = shinqlx.Teams.RED\n\
         blue = shinqlx.Teams.BLUE\n\
         team = shinqlx.Teams.RED\n\
         team_int = 1\n\
         TEAM_RED = shinqlx.TEAM_RED\n\
         low = shinqlx.Priorities.LOW\n\
         moderator = shinqlx.Permissions.MODERATOR\n\
         admin = shinqlx.Permissions.GAME_ADMIN\n\
         stop = shinqlx.ReturnCodes.STOP\n\
         none = shinqlx.ReturnCodes.NONE\n\
         state = shinqlx.GameStates.WARMUP\n\
         holdable = shinqlx.Holdable.TELEPORTER\n\
         weapon = shinqlx.Weapon.ROCKET_LAUNCHER\n\
         by_team = {red: 1, blue: 2}\n",
        &[
            ("empty_loop", "pass"),
            ("int_equals_int", "team_int == TEAM_RED"),
            ("enum_equals_int", "team == TEAM_RED"),
            ("enum_equals_enum", "team == red"),
            ("enum_differs_from_enum", "team == blue"),
            ("enum_equals_string", "team == \"red\""),
            ("enum_attribute_lookup", "team == shinqlx.Teams.RED"),
            ("enum_hash", "hash(team)"),
            ("enum_as_dict_key", "by_team[team]"),
            ("priority_equals_int", "low == shinqlx.PRI_LOW"),
            ("permission_ordering", "admin >= moderator"),
            ("return_code_equals_none", "none == None"),
            ("return_code_is_falsy", "not none"),
            ("return_code_truthy", "not stop"),
            ("game_state_equals_string", "state == \"warmup\""),
            ("holdable_equals_string", "holdable == \"teleporter\""),
            ("holdable_equals_int", "holdable == 27"),
            ("weapon_equals_string", "weapon == \"rl\""),
            ("weapon_to_string", "str(weapon)"),
        ],
        ITERATIONS,
    );
}

#[criterion]
fn events_benchmark(c: &mut Criterion) {
    python_case_group(
        c,
        "events",
        "dispatchers = shinqlx.EVENT_DISPATCHERS\n\
         by_name = {\"frame\": 1}\n\
         frame = shinqlx.Events.FRAME\n",
        &[
            ("dispatcher_by_string", "dispatchers[\"frame\"]"),
            ("dispatcher_by_member", "dispatchers[frame]"),
            (
                "dispatcher_by_member_attribute",
                "dispatchers[shinqlx.Events.FRAME]",
            ),
            ("dict_by_string", "by_name[\"frame\"]"),
            ("dict_by_member", "by_name[frame]"),
            ("member_equals_string", "frame == \"frame\""),
        ],
        ITERATIONS,
    );
}

#[criterion]
fn name_of_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("name_of");
    group.throughput(Throughput::Elements(ITERATIONS as u64));
    for (name, expression) in [
        ("string", "\"red\""),
        ("team", "shinqlx.Teams.RED"),
        ("privileges", "shinqlx.Privileges.MOD"),
        ("holdable", "shinqlx.Holdable.TELEPORTER"),
    ] {
        let case = bench_support::NameOfCase::new(expression);
        group.bench_function(name, |b| b.iter(|| case.run(black_box(ITERATIONS))));
    }
    group.finish();
}

#[criterion]
fn cvar_attribute_benchmark(c: &mut Criterion) {
    python_case_group(
        c,
        "cvar_attribute",
        "class bench_plugin(shinqlx.Plugin):\n\
         \x20   number = shinqlx.Cvar(\"qlx_benchNumber\", 3)\n\
         \x20   text = shinqlx.Cvar(\"qlx_benchText\", \"text\")\n\
         \x20   names = shinqlx.Cvar(\"qlx_benchNames\", [\"a\", \"b\"])\n\
         \x20   plain = 3\n\
         \n\
         plugin = bench_plugin()\n",
        &[
            ("plain_attribute", "plugin.plain"),
            ("cvar_int_unset", "plugin.number"),
            ("cvar_str_unset", "plugin.text"),
            ("cvar_list_unset", "plugin.names"),
            (
                "get_cvar_no_type_unset",
                "plugin.get_cvar(\"qlx_benchNumber\")",
            ),
            (
                "get_cvar_str_unset",
                "plugin.get_cvar(\"qlx_benchNumber\", str)",
            ),
            (
                "get_cvar_int_unset",
                "plugin.get_cvar(\"qlx_benchNumber\", int)",
            ),
            (
                "get_cvar_list_unset",
                "plugin.get_cvar(\"qlx_benchNumber\", list)",
            ),
            ("bound_method_only", "plugin.get_cvar"),
        ],
        ITERATIONS,
    );
}

#[criterion]
fn plugin_declaration_benchmark(c: &mut Criterion) {
    let hooked = "compile('class hooked(shinqlx.Plugin):\\n' + ''.join(\
        f'    @shinqlx.hook(shinqlx.Events.CHAT)\\n    def hook_{i}(self, player, msg, channel):\\n        pass\\n\\n'\
        for i in range(20)), 'hooked', 'exec')\n";
    let plain = "compile('class plain(shinqlx.Plugin):\\n' + ''.join(\
        f'    def hook_{i}(self, player, msg, channel):\\n        pass\\n\\n'\
        for i in range(20)), 'plain', 'exec')\n";
    let commands = "compile('class commands(shinqlx.Plugin):\\n' + ''.join(\
        f'    @shinqlx.command(\"cmd{i}\", permission=1)\\n    def cmd_{i}(self, player, msg, channel):\\n        pass\\n\\n'\
        for i in range(20)), 'commands', 'exec')\n";
    python_case_group(
        c,
        "plugin_declaration",
        &format!("hooked = {hooked}plain = {plain}commands = {commands}"),
        &[
            (
                "class_with_20_methods",
                "exec(plain, {\"shinqlx\": shinqlx})",
            ),
            (
                "class_with_20_hook_decorators",
                "exec(hooked, {\"shinqlx\": shinqlx})",
            ),
            (
                "class_with_20_command_decorators",
                "exec(commands, {\"shinqlx\": shinqlx})",
            ),
        ],
        10,
    );

    let mut group = c.benchmark_group("declared_cvars");
    for (name, cvars, methods) in [("none_30_methods", 0, 30), ("five_30_methods", 5, 30)] {
        let case = bench_support::DeclaredCvarsCase::new(cvars, methods);
        group.bench_function(name, |b| b.iter(|| case.register()));
    }
    group.finish();
}

#[criterion]
fn more_events_benchmark(c: &mut Criterion) {
    bench_support::prepare_python();

    let mut group = c.benchmark_group("more_events");
    group.bench_function("client_command_say", |b| {
        b.iter(|| bench_support::handle_client_command(black_box(3), black_box("say \"hello\"")))
    });
    group.bench_function("client_command_callvote", |b| {
        b.iter(|| {
            bench_support::handle_client_command(
                black_box(3),
                black_box("callvote map campgrounds"),
            )
        })
    });
    group.bench_function("client_command_vote", |b| {
        b.iter(|| bench_support::handle_client_command(black_box(3), black_box("vote y")))
    });
    group.bench_function("console_print", |b| {
        b.iter(|| bench_support::handle_console_print(black_box("broadcast: print \"text\"\n")))
    });
    for (name, message) in [
        (
            "stats_player_death",
            r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#,
        ),
        (
            "stats_round_over",
            r#"{"DATA": {"MATCH_GUID": "asdf", "ROUND": 10, "TEAM_WON": "RED", "TIME": 539, "WARMUP": false}, "TYPE": "ROUND_OVER"}"#,
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| bench_support::handle_stats_message_draining(black_box(message)))
        });
    }
    group.finish();
}

#[criterion]
fn lookup_call_benchmark(c: &mut Criterion) {
    python_case_group(
        c,
        "lookup_call",
        "plugin = shinqlx.Plugin()\n",
        &[
            ("plugin_clean_text", "plugin.clean_text(\"^1Red^7Player\")"),
            (
                "plugin_clean_text_plain",
                "plugin.clean_text(\"RedPlayer\")",
            ),
            ("plugin_clean_text_empty", "plugin.clean_text(\"\")"),
            ("plugin_clean_text_bound", "plugin.clean_text"),
            (
                "python_re_sub",
                "shinqlx.re_color_tag.sub(\"\", \"^1Red^7Player\")",
            ),
            ("plugin_is_vote_active", "plugin.is_vote_active()"),
            ("uptime", "shinqlx.uptime()"),
        ],
        ITERATIONS,
    );
}
