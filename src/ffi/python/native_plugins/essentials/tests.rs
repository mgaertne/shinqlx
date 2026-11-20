use core::{ffi::CStr, sync::atomic::Ordering};

use mockall::predicate;
use pyo3::{prelude::*, types::PyDict};
use rstest::rstest;

use super::{
    Essentials,
    mappool::{mappool_listing, parse_mappool},
    player_key,
    sounds::SOUNDS_FLAG,
};
use crate::{
    ffi::{
        c::prelude::{
            CS_SERVERINFO, CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES, MockCurrentLevel, team_t,
        },
        python::{
            AbstractChannel, CHAT_CHANNEL, Player, PlayerInfo, Plugin, PythonReturnCodes,
            TeamChatChannel,
            native_plugins::test_support::{
                FakeRedis, REDIS_5, ServerCommands, fixture_module, real_player,
                real_player_with_id,
            },
            plugin::PluginMethods,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{
                RECORDED_VOTE_STATES, RestoredAttribute, default_test_player_info,
                engine_with_cvars, with_connected_teamed_players,
            },
        },
    },
    prelude::*,
};

const STOP_ALL: i32 = PythonReturnCodes::RET_STOP_ALL as i32;
const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;
const CALLER_ID: i64 = 1001;
const TARGET_ID: i64 = 1003;

const FIXTURES: &CStr = c"
import shinqlx


caller = None
target = None
online = []


def seat(new_caller, new_target):
    global caller, target
    caller = new_caller
    target = new_target
    online[:] = [caller, target]

console = []
redirected = []
owner_id = [76561198000000009]


def fake_owner():
    return owner_id[0]


def fake_console_command(command):
    console.append(command)
    if command == 'fail':
        raise RuntimeError('boom')


class Redirect:
    def __init__(self, channel):
        redirected.append('created')

    def __enter__(self):
        redirected.append('entered')

    def __exit__(self, *args):
        redirected.append('left')

";

struct Fixtures<'py> {
    plugin: Bound<'py, Essentials>,
    module: Bound<'py, PyModule>,
    database: Bound<'py, PyAny>,
    channel: Bound<'py, AbstractChannel>,
    caller: Bound<'py, Player>,
    target: Bound<'py, Player>,
    commands: ServerCommands,
}

impl Fixtures<'_> {
    fn replies(&self) -> Vec<String> {
        self.channel
            .getattr("replies")
            .and_then(|replies| replies.extract())
            .expect("could not get the replies")
    }

    fn told(&self, _player: &Bound<'_, Player>) -> Vec<String> {
        self.commands.printed(self.plugin.py())
    }

    fn strings(&self, name: &str) -> Vec<String> {
        self.module
            .getattr(name)
            .and_then(|values| values.extract())
            .expect("could not get the strings")
    }

    fn forced(&self) -> Vec<bool> {
        RECORDED_VOTE_STATES.lock().clone()
    }

    fn store(&self, key: &str, value: &str) {
        self.database
            .getattr("strings")
            .and_then(|strings| strings.set_item(key, value))
            .expect("could not store the value");
    }

    fn stored(&self, key: &str) -> Option<String> {
        self.database
            .getattr("strings")
            .and_then(|strings| strings.call_method1("get", (key,)))
            .and_then(|value| value.extract())
            .expect("could not read the value")
    }

    fn names(&self, steam_id: i64) -> Vec<String> {
        self.database
            .getattr("lists")
            .and_then(|lists| {
                lists.call_method1("get", (player_key(steam_id), Vec::<String>::new()))
            })
            .and_then(|names| names.extract())
            .expect("could not read the names")
    }

    fn set_members(&self, key: &str) -> Vec<String> {
        self.database
            .getattr("sets")
            .and_then(|sets| sets.call_method1("get", (key, Vec::<String>::new())))
            .and_then(|members| {
                self.database
                    .py()
                    .import("builtins")?
                    .call_method1("sorted", (members,))?
                    .extract::<Vec<String>>()
            })
            .expect("could not read the set")
    }
}

fn with_essentials<F>(cvars: &[(&'static str, &'static CStr)], mappool: Option<&str>, test: F)
where
    F: FnOnce(Fixtures<'_>),
{
    with_essentials_engine(engine_with_cvars(cvars), mappool, test);
}

fn with_vote(
    engine: MockEngineBuilder,
    active: bool,
    counts: Option<(i64, i64)>,
) -> MockEngineBuilder {
    let (yes, no) = counts.map_or_else(Default::default, |(yes, no)| {
        (yes.to_string(), no.to_string())
    });
    engine
        .with_get_configstring(
            CS_VOTE_STRING as u16,
            if active { "map campgrounds" } else { "" },
            0..,
        )
        .with_get_configstring(CS_VOTE_YES as u16, yes, 0..)
        .with_get_configstring(CS_VOTE_NO as u16, no, 0..)
}

fn with_game(engine: MockEngineBuilder, state: Option<&str>) -> MockEngineBuilder {
    let serverinfo = state.map_or_else(String::new, |state| {
        format!(r"\g_gameState\{state}\g_factory\ca\mapname\campgrounds")
    });
    engine.with_get_configstring(CS_SERVERINFO as u16, serverinfo, 0..)
}

const IN_TEAMS: [(i32, &str, team_t); 2] = [
    (1, "Caller^7", team_t::TEAM_RED),
    (3, "Eve^7", team_t::TEAM_BLUE),
];

fn with_essentials_engine<F>(engine: MockEngineBuilder, mappool: Option<&str>, test: F)
where
    F: FnOnce(Fixtures<'_>),
{
    with_essentials_players(engine, &IN_TEAMS, mappool, test);
}

fn with_essentials_players<F>(
    engine: MockEngineBuilder,
    players: &[(i32, &'static str, team_t)],
    mappool: Option<&str>,
    test: F,
) where
    F: FnOnce(Fixtures<'_>),
{
    let engine = engine
        .with_get_configstring(CS_VOTE_STRING as u16, "", 0..)
        .with_get_configstring(CS_VOTE_YES as u16, "", 0..)
        .with_get_configstring(CS_VOTE_NO as u16, "", 0..)
        .with_get_configstring(CS_SERVERINFO as u16, "", 0..);
    with_connected_teamed_players(engine, players, || {
        let vote_time = MockCurrentLevel::try_get_context();
        vote_time.expect().returning(|| {
            let mut level = MockCurrentLevel::new();
            level.expect_get_vote_time().return_const(21);
            Ok(level)
        });
        RECORDED_VOTE_STATES.lock().clear();
        let commands = ServerCommands::capture();
        Python::attach(|py| {
            let _fake_redis = FakeRedis::install(py, REDIS_5);
            let module = fixture_module(py, FIXTURES);
            let plugin_class = py.get_type::<Plugin>();
            let _restore_database = RestoredAttribute::capture(plugin_class.as_any(), "database");
            plugin_class
                .setattr("database", module.getattr("Database").expect("no Database"))
                .expect("could not set the database");
            let shinqlx = py.import("shinqlx").expect("no shinqlx");
            let _restored_functions = [
                ("owner", "fake_owner"),
                ("console_command", "fake_console_command"),
                ("redirect_print", "Redirect"),
            ]
            .map(|(name, fake)| {
                let restored = RestoredAttribute::capture(shinqlx.as_any(), name);
                shinqlx
                    .setattr(name, module.getattr(fake).expect("missing fake"))
                    .expect("could not replace the function");
                restored
            });

            let plugin = Bound::new(
                py,
                PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Essentials {
                    vote_count: Default::default(),
                    last_vote: Default::default(),
                    recent_commands: Default::default(),
                    recent_disconnects: Default::default(),
                    mappool: mappool.map(parse_mappool),
                }),
            )
            .expect("could not create the plugin");
            let database = plugin.as_super().get_db().expect("no database");
            let get = |name: &str| module.getattr(name).expect("missing fixture");
            let caller = real_player_with_id(py, 1, CALLER_ID, "Caller^7", "1.1.1.1");
            let target = real_player_with_id(py, 3, TARGET_ID, "Eve^7", "");
            for (player, team) in [(&caller, team_t::TEAM_RED), (&target, team_t::TEAM_BLUE)] {
                *player.get().player_info.write() = PlayerInfo {
                    team: team as i32,
                    ..default_test_player_info()
                };
            }
            get("seat")
                .call1((&caller, &target))
                .expect("could not seat the players");
            test(Fixtures {
                plugin,
                database,
                channel: get("Channel")
                    .call0()
                    .and_then(|channel| channel.cast_into::<AbstractChannel>().map_err(Into::into))
                    .expect("no channel"),
                caller,
                target,
                commands,
                module: module.clone(),
            });
        });
    });
}

fn msg(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

const VOTE_CVARS: [(&str, &CStr); 6] = [
    ("qlx_votepass", c"1"),
    ("qlx_votepassThreshold", c"0.5"),
    ("qlx_teamsizeMinimum", c"2"),
    ("qlx_teamsizeMaximum", c"6"),
    ("qlx_enforceMappool", c"1"),
    ("qlx_commandPrefix", c"!"),
];

const NO_VOTEPASS: [(&str, &CStr); 6] = [
    ("qlx_votepass", c"0"),
    ("qlx_votepassThreshold", c"0.5"),
    ("qlx_teamsizeMinimum", c"2"),
    ("qlx_teamsizeMaximum", c"6"),
    ("qlx_enforceMappool", c"1"),
    ("qlx_commandPrefix", c"!"),
];

const MAPPOOL: &str = "# comment\ncampgrounds|ca\nCampgrounds|ffa\n  overkill | ca\nnot a line\n";

#[test]
fn the_mappool_file_is_parsed_into_maps_and_factories() {
    let mappool = parse_mappool(MAPPOOL);

    assert_eq!(mappool.len(), 2);
    assert_eq!(mappool["campgrounds"], ["ca", "ffa"]);
    assert_eq!(mappool["overkill"], ["ca"]);
    assert_eq!(
        mappool_listing(&mappool, 2),
        "  campgrounds               Factories: ca, ffa\n  overkill                  Factories: ca"
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn connecting_players_are_added_to_the_player_ip_and_name_lists(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let caller = real_player(fixtures.plugin.py(), CALLER_ID, "Caller^7", "1.1.1.1");
        let result = Essentials::handle_player_connect(&fixtures.plugin, &caller);

        assert!(result.is_ok());
        assert_eq!(fixtures.names(CALLER_ID), ["Caller^7"]);
        assert_eq!(
            fixtures.set_members("minqlx:players"),
            [CALLER_ID.to_string()]
        );
        assert_eq!(fixtures.set_members("minqlx:ips"), ["1.1.1.1"]);
        assert_eq!(
            fixtures.set_members("minqlx:ips:1.1.1.1"),
            [CALLER_ID.to_string()]
        );
        assert_eq!(
            fixtures.set_members(&format!("{}:ips", player_key(CALLER_ID))),
            ["1.1.1.1"]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn players_without_an_ip_are_only_added_to_the_player_list(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let target = real_player(fixtures.plugin.py(), TARGET_ID, "Eve^7", "");
        let result = Essentials::handle_player_connect(&fixtures.plugin, &target);

        assert!(result.is_ok());
        assert_eq!(fixtures.names(TARGET_ID), ["Eve^7"]);
        assert!(fixtures.set_members("minqlx:ips").is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn known_players_only_get_new_names_remembered(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let target = real_player(fixtures.plugin.py(), TARGET_ID, "Eve^7", "");
        Essentials::handle_player_connect(&fixtures.plugin, &target).expect("first connect failed");
        Essentials::handle_player_connect(&fixtures.plugin, &target)
            .expect("second connect failed");
        assert_eq!(fixtures.names(TARGET_ID), ["Eve^7"]);

        *target.get().name.write() = "^1Eve2^7".to_owned();
        let result = Essentials::handle_player_connect(&fixtures.plugin, &target);

        assert!(result.is_ok());
        assert_eq!(fixtures.names(TARGET_ID), ["^1Eve2^7", "Eve^7"]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn disconnecting_players_are_remembered_and_stamped(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let target = real_player(py, TARGET_ID, "Eve^7", "");

        let result =
            Essentials::handle_player_disconnect(&fixtures.plugin, &target, py.None().bind(py));

        assert!(result.is_ok());
        assert_eq!(fixtures.plugin.get().recent_disconnects.lock().len(), 1);
        let stamp = fixtures
            .stored(&format!("{}:last_seen", player_key(TARGET_ID)))
            .expect("no last seen");
        assert_eq!(stamp.len(), "2026-10-27 12:00:00".len());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn only_the_ten_latest_disconnects_are_kept(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let target = real_player(py, TARGET_ID, "Eve^7", "");
        for _ in 0..12 {
            Essentials::handle_player_disconnect(&fixtures.plugin, &target, py.None().bind(py))
                .expect("disconnect failed");
        }

        assert_eq!(fixtures.plugin.get().recent_disconnects.lock().len(), 10);
    });
}

#[rstest]
#[case("8", "The team size is larger than what the server allows.")]
#[case("1", "The team size is smaller than what the server allows.")]
#[cfg_attr(miri, ignore)]
#[serial]
fn teamsize_votes_beyond_the_limits_are_stopped(
    _pyshinqlx_setup: (),
    #[case] size: &str,
    #[case] expected: &str,
) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        let result =
            Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "TeamSize", size);

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), [expected]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn teamsize_votes_that_are_no_numbers_are_ignored(_pyshinqlx_setup: ()) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        let result =
            Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "teamsize", "x");

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.plugin.get().vote_count.load(Ordering::SeqCst), 0);
    });
}

#[rstest]
#[case("", &["Available maps and factories:", "  campgrounds               Factories: ca, ffa\n  overkill                  Factories: ca"])]
#[case("unknown", &["This map is not allowed. Use !mappool to see available options."])]
#[case("overkill ffa", &["This factory is not allowed on that map. Use !mappool to see available options."])]
#[cfg_attr(miri, ignore)]
#[serial]
fn map_votes_outside_of_the_mappool_are_stopped(
    _pyshinqlx_setup: (),
    #[case] args: &str,
    #[case] expected: &[&str],
) {
    with_essentials(&VOTE_CVARS, Some(MAPPOOL), |fixtures| {
        let result =
            Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "map", args);

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), expected);
    });
}

#[rstest]
#[case("Campgrounds ffa")]
#[case("overkill")]
#[cfg_attr(miri, ignore)]
#[serial]
fn map_votes_inside_of_the_mappool_pass_on(_pyshinqlx_setup: (), #[case] args: &str) {
    with_essentials(&NO_VOTEPASS, Some(MAPPOOL), |fixtures| {
        let result =
            Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "map", args);

        assert_eq!(result.ok(), Some(None));
        assert!(fixtures.told(&fixtures.caller).is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn votes_are_passed_automatically_after_the_delay(_pyshinqlx_setup: ()) {
    let engine = with_vote(engine_with_cvars(&VOTE_CVARS), true, Some((2, 0)));
    with_essentials_engine(engine, None, |fixtures| {
        let py = fixtures.plugin.py();
        let scheduler = py
            .import("shinqlx")
            .and_then(|shinqlx| shinqlx.getattr("frame_tasks"))
            .expect("no frame tasks");
        let before = scheduler
            .getattr("queue")
            .and_then(|queue| queue.len())
            .expect("could not count");

        let result =
            Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "kick", "Eve");

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.plugin.get().last_vote.load(Ordering::SeqCst), 0);
        assert_eq!(fixtures.plugin.get().vote_count.load(Ordering::SeqCst), 1);
        let queue = scheduler.getattr("queue").expect("no queue");
        assert_eq!(queue.len().expect("no length"), before + 1);
        let task = queue.get_item(before).expect("no task");
        let delay = task.getattr("time").and_then(|time| time.extract::<f64>());
        assert!(delay.is_ok());
        let arguments = task.getattr("argument").expect("no arguments");
        task.getattr("action")
            .and_then(|action| {
                action.call(
                    arguments
                        .cast::<pyo3::types::PyTuple>()
                        .expect("not a tuple"),
                    None,
                )
            })
            .expect("could not run the task");
        scheduler
            .call_method1("cancel", (task,))
            .expect("could not cancel the task");
        assert_eq!(fixtures.forced(), [true, true]);
    });
}

#[rstest]
#[case(false, Some((3, 0)), 0.5, false)]
#[case(true, None, 0.5, false)]
#[case(true, Some((1, 1)), 0.5, false)]
#[case(true, Some((0, 2)), 0.5, false)]
#[case(true, Some((1, 0)), 0.6, false)]
#[case(true, Some((2, 0)), 0.5, true)]
#[case(true, Some((1, 0)), 0.0, true)]
#[cfg_attr(miri, ignore)]
#[serial]
fn a_vote_is_only_forced_when_it_is_leading_and_enough_players_took_part(
    _pyshinqlx_setup: (),
    #[case] active: bool,
    #[case] counts: Option<(i64, i64)>,
    #[case] require: f64,
    #[case] forced: bool,
) {
    let engine = with_vote(engine_with_cvars(&NO_VOTEPASS), active, counts);
    with_essentials_engine(engine, None, |fixtures| {
        let result = Essentials::force(&fixtures.plugin, require, 0);

        assert!(result.is_ok());
        assert_eq!(fixtures.forced().len(), 2 * usize::from(forced));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn an_outdated_vote_is_not_forced(_pyshinqlx_setup: ()) {
    let engine = with_vote(engine_with_cvars(&NO_VOTEPASS), true, Some((2, 0)));
    with_essentials_engine(engine, None, |fixtures| {
        fixtures.plugin.get().last_vote.store(5, Ordering::SeqCst);

        let result = Essentials::force(&fixtures.plugin, 0.0, 4);

        assert!(result.is_ok());
        assert!(fixtures.forced().is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn nobody_in_the_teams_means_no_forced_vote(_pyshinqlx_setup: ()) {
    let spectators = [
        (1, "Caller^7", team_t::TEAM_SPECTATOR),
        (3, "Eve^7", team_t::TEAM_SPECTATOR),
    ];
    let engine = with_vote(engine_with_cvars(&NO_VOTEPASS), true, Some((2, 0)));
    with_essentials_players(engine, &spectators, None, |fixtures| {
        let result = Essentials::force(&fixtures.plugin, 0.5, 0);

        assert!(result.is_ok());
        assert!(fixtures.forced().is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn the_latest_commands_are_recorded_and_listed_without_the_current_one(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let command = py.None();
        for args in ["first", "second", "listing"] {
            Essentials::handle_command(&fixtures.plugin, &fixtures.caller, command.bind(py), args)
                .expect("could not record the command");
        }

        let result = Essentials::cmd_commands(
            &fixtures.plugin,
            &fixtures.target,
            msg(&["commands"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.target),
            [
                "The most recent ^62^7 commands executed:",
                "  Caller^7 executed: second",
                "  Caller^7 executed: first"
            ]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn only_eleven_commands_are_recorded(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let command = py.None();
        for _ in 0..15 {
            Essentials::handle_command(&fixtures.plugin, &fixtures.caller, command.bind(py), "x")
                .expect("could not record the command");
        }

        assert_eq!(fixtures.plugin.get().recent_commands.lock().len(), 11);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn no_recorded_commands_are_reported(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_commands(
            &fixtures.plugin,
            &fixtures.target,
            msg(&["commands"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.target),
            ["No commands have been recorded yet."]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn no_disconnects_are_reported(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_disconnects(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["dcs"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            ["No players have disconnected yet."]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn recent_disconnects_are_listed(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let target = real_player(py, TARGET_ID, "Eve^7", "");
        Essentials::handle_player_disconnect(&fixtures.plugin, &target, py.None().bind(py))
            .expect("disconnect failed");

        let result = Essentials::cmd_disconnects(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["dcs"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            [
                "The most recent ^61^7 player disconnects:".to_owned(),
                format!("  Eve^7 ({TARGET_ID}): ^60^7 seconds ago")
            ]
        );
    });
}

#[rstest]
#[case(&["id"], &["All connected players:", "  1^6:^7 Caller^7\n  3^6:^7 Eve^7"])]
#[case(&["id", "eve"], &["A total of ^61^7 players matched:", "  3^6:^7 Eve^7"])]
#[case(&["id", "e", "call"], &["A total of ^62^7 players matched:", "  1^6:^7 Caller^7\n  3^6:^7 Eve^7"])]
#[case(&["id", "zzz"], &["Sorry, but no players matched your tokens."])]
#[cfg_attr(miri, ignore)]
#[serial]
fn id_lists_the_players_matching_the_tokens(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected: &[&str],
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_id(
            &fixtures.plugin,
            &fixtures.caller,
            msg(words),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), expected);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn players_prints_a_table_like_the_console_command(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_players(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["players"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            [concat!(
                "ID |     SteamID64     |   IP Address    | Name\n",
                " 1 |              1001 |                 | Caller^7\n",
                " 3 |              1003 |                 | Eve^7\n"
            )]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn db_without_a_key_asks_for_usage(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_db(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["db"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(USAGE)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn db_reports_missing_keys(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_db(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["db", "nothing"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        assert_eq!(
            fixtures.replies(),
            ["The key is not present in the database."]
        );
    });
}

#[rstest]
#[case("string", "value")]
#[case("list", "['b', 'a']")]
#[case("set", "{'x'}")]
#[case("zset", "[('m', 2.0)]")]
#[case("hash", "{'k': 'v'}")]
#[cfg_attr(miri, ignore)]
#[serial]
fn db_prints_the_value_depending_on_its_type(
    _pyshinqlx_setup: (),
    #[case] kind: &str,
    #[case] expected: &str,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        let store = |attribute: &str, value: Bound<'_, PyAny>| {
            fixtures
                .database
                .getattr(attribute)
                .and_then(|items| items.set_item("key", value))
                .expect("could not store");
        };
        match kind {
            "string" => store(
                "strings",
                "value".into_pyobject(py).expect("str").into_any(),
            ),
            "list" => store(
                "lists",
                vec!["b", "a"].into_pyobject(py).expect("list").into_any(),
            ),
            "set" => store(
                "sets",
                pyo3::types::PySet::new(py, ["x"]).expect("set").into_any(),
            ),
            "zset" => {
                let scores = PyDict::new(py);
                scores.set_item("m", 2.0).expect("score");
                store("zsets", scores.into_any());
            }
            _ => {
                let hash = PyDict::new(py);
                hash.set_item("k", "v").expect("field");
                store("hashes", hash.into_any());
            }
        }

        let result = Essentials::cmd_db(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["db", "key"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.replies(), [expected]);
    });
}

#[rstest]
#[case(&["seen"], Some(USAGE), &[])]
#[case(&["seen", "abc"], None, &["Unintelligible SteamID64."])]
#[case(&["seen", "12"], None, &["Invalid SteamID64."])]
#[case(&["seen", "1003"], None, &["That would be Eve^7^7, who is currently on this very server!"])]
#[case(&["seen", "76561198000000042"], None, &["^7I have never seen that player before."])]
#[case(&["seen", "76561198000000009"], None, &["^7I have never seen my ^6master^7 before."])]
#[cfg_attr(miri, ignore)]
#[serial]
fn seen_reports_where_a_steam_id_was_last_seen(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected_code: Option<i32>,
    #[case] expected: &[&str],
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_seen(
            &fixtures.plugin,
            &fixtures.caller,
            msg(words),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(expected_code));
        assert_eq!(fixtures.replies(), expected);
    });
}

#[rstest]
#[case("1970-01-01 00:00:00", false)]
#[case("", true)]
#[cfg_attr(miri, ignore)]
#[serial]
fn seen_tells_how_long_ago_a_player_was_seen(
    _pyshinqlx_setup: (),
    #[case] stamp: &str,
    #[case] recent: bool,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let stamp = if recent {
            crate::ffi::python::native_plugins::timed_entries::local_time(
                crate::ffi::python::native_plugins::timed_entries::now() - 3_900.0,
            )
            .expect("no time")
        } else {
            stamp.to_owned()
        };
        fixtures.store(
            &format!("{}:last_seen", player_key(76_561_198_000_000_042)),
            &stamp,
        );

        let result = Essentials::cmd_seen(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["seen", "76561198000000042"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        let replies = fixtures.replies();
        assert_eq!(replies.len(), 1);
        if recent {
            assert_eq!(
                replies[0],
                "^7I saw that player ^61^7 hour(s) and ^65^7 minute(s) ago."
            );
        } else {
            assert!(replies[0].contains("day(s)"));
        }
    });
}

#[rstest]
#[case(&["time", "x"], "Unintelligible time zone offset.")]
#[case(&["time", "24"], "Unintelligible time zone offset.")]
#[cfg_attr(miri, ignore)]
#[serial]
fn time_rejects_unintelligible_offsets(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected: &str,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_time(
            &fixtures.plugin,
            &fixtures.caller,
            msg(words),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.replies(), [expected]);
    });
}

#[rstest]
#[case(&["time", "0"], "UTC")]
#[case(&["time", "5"], "UTC+5")]
#[case(&["time", "-3"], "UTC-3")]
#[case(&["time"], "UTC")]
#[cfg_attr(miri, ignore)]
#[serial]
fn time_reports_the_time_in_the_requested_zone(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] zone: &str,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_time(
            &fixtures.plugin,
            &fixtures.caller,
            msg(words),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        let replies = fixtures.replies();
        assert_eq!(replies.len(), 1);
        assert!(replies[0].starts_with("The current time is: ^6"));
        if words.len() > 1 {
            assert!(replies[0].ends_with(zone));
        } else {
            assert!(replies[0].contains(" UTC"));
        }
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn time_uses_the_local_offset_by_default(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let offset = chrono::Local::now().offset().local_minus_utc() / 3600;
        let suffix = match offset {
            0 => " UTC".to_owned(),
            offset if offset > 0 => format!(" UTC+{offset}"),
            offset => format!(" UTC{offset}"),
        };

        let result = Essentials::cmd_time(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["time"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        let replies = fixtures.replies();
        assert_eq!(replies.len(), 1);
        assert!(replies[0].ends_with(&suffix), "{replies:?}");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn db_reports_the_error_of_a_failing_lookup_and_passes_it_on(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let py = fixtures.plugin.py();
        fixtures
            .database
            .setattr(
                "type",
                py.eval(
                    c"lambda key: (_ for _ in ()).throw(ValueError('boom'))",
                    None,
                    None,
                )
                .expect("could not build the failing lookup"),
            )
            .expect("could not replace type");

        let result = Essentials::cmd_db(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["db", "key"]),
            &fixtures.channel,
        );

        assert!(result.is_err_and(|err| err.is_instance_of::<pyo3::exceptions::PyValueError>(py)));
        assert_eq!(fixtures.replies(), ["^1ValueError^7: boom"]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn help_tells_about_the_version(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_help(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["help"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        let told = fixtures.told(&fixtures.caller);
        assert_eq!(told.len(), 2);
        assert!(told[0].starts_with("shinqlx: ^6"));
        assert!(told[1].contains("github.com/mgaertne/shinqlx"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn rcon_without_a_command_asks_for_usage(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_rcon(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["rcon"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(USAGE)));
    });
}

#[rstest]
#[case("status", true)]
#[case("fail", false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn rcon_runs_the_command_with_the_output_redirected(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] succeeds: bool,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_rcon(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["rcon", command]),
            &fixtures.channel,
        );

        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(fixtures.strings("console"), [command]);
        assert_eq!(
            fixtures.strings("redirected"),
            ["created", "entered", "left"]
        );
    });
}

#[rstest]
#[case(None, "No map pool is being enforced. You are free to vote any map.")]
#[case(Some(MAPPOOL), "  campgrounds")]
#[cfg_attr(miri, ignore)]
#[serial]
fn mappool_lists_the_enforced_pool(
    _pyshinqlx_setup: (),
    #[case] mappool: Option<&str>,
    #[case] expected: &str,
) {
    with_essentials(&VOTE_CVARS, mappool, |fixtures| {
        let result = Essentials::cmd_mappool(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["mappool"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        let told = fixtures.told(&fixtures.caller);
        assert_eq!(told.len(), 1);
        if mappool.is_some() {
            assert!(told[0].starts_with("campgrounds"));
        } else {
            assert_eq!(told[0], expected);
        }
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn mappool_is_not_listed_when_it_is_not_enforced(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS_UNENFORCED, Some(MAPPOOL), |fixtures| {
        let result = Essentials::cmd_mappool(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["mappool"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            ["No map pool is being enforced. You are free to vote any map."]
        );
    });
}

const NO_VOTEPASS_UNENFORCED: [(&str, &CStr); 6] = [
    ("qlx_votepass", c"0"),
    ("qlx_votepassThreshold", c"0.5"),
    ("qlx_teamsizeMinimum", c"2"),
    ("qlx_teamsizeMaximum", c"6"),
    ("qlx_enforceMappool", c"0"),
    ("qlx_commandPrefix", c"!"),
];

type Handler = fn(
    &Bound<'_, Essentials>,
    &Bound<'_, Player>,
    Vec<String>,
    &Bound<'_, AbstractChannel>,
) -> PyResult<Option<i32>>;

fn handler(name: &str) -> Handler {
    match name {
        "kick" => Essentials::cmd_kick,
        "kickban" => Essentials::cmd_kickban,
        "red" => Essentials::cmd_red,
        "blue" => Essentials::cmd_blue,
        "spectate" => Essentials::cmd_spectate,
        "free" => Essentials::cmd_free,
        "addmod" => Essentials::cmd_addmod,
        "addadmin" => Essentials::cmd_addadmin,
        "demote" => Essentials::cmd_demote,
        "mute" => Essentials::cmd_mute,
        "unmute" => Essentials::cmd_unmute,
        "slap" => Essentials::cmd_slap,
        "slay" => Essentials::cmd_slay,
        "sound" => Essentials::cmd_sound,
        "music" => Essentials::cmd_music,
        "random" => Essentials::cmd_random,
        "switch" => Essentials::cmd_switch,
        "map" => Essentials::cmd_map,
        "teamsize" => Essentials::cmd_teamsize,
        other => panic!("no handler for {other}"),
    }
}

impl Fixtures<'_> {
    fn run(&self, name: &str, caller: &Bound<'_, Player>, words: &[&str]) -> Option<Option<i32>> {
        handler(name)(&self.plugin, caller, msg(words), &self.channel).ok()
    }
}

#[rstest]
#[case("kick", &["kick", "3", "bad", "manners"], None, Some("bad manners"))]
#[case("kick", &["kick", "3"], None, Some(""))]
#[case("kickban", &["kickban", "3"], Some("tempban 3"), None)]
#[case("red", &["red", "3"], Some("put 3 red"), None)]
#[case("blue", &["blue", "3"], Some("put 3 blue"), None)]
#[case("spectate", &["spec", "3"], Some("put 3 spectator"), None)]
#[case("free", &["free", "3"], Some("put 3 free"), None)]
#[case("addmod", &["addmod", "3"], Some("addmod 3"), None)]
#[case("addadmin", &["addadmin", "3"], Some("addadmin 3"), None)]
#[case("demote", &["demote", "3"], Some("demote 3"), None)]
#[case("mute", &["mute", "3"], Some("mute 3"), None)]
#[case("unmute", &["unmute", "3"], Some("unmute 3"), None)]
#[cfg_attr(miri, ignore)]
#[serial]
fn player_commands_act_on_the_resolved_player(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] words: &[&str],
    #[case] console_command: Option<&'static str>,
    #[case] dropped_for: Option<&str>,
) {
    let engine = console_command.map_or_else(
        || engine_with_cvars(&NO_VOTEPASS),
        |expected| engine_with_cvars(&NO_VOTEPASS).with_execute_console_command(expected, 1),
    );
    with_essentials_engine(engine, None, |fixtures| {
        let result = fixtures.run(command, &fixtures.caller, words);

        assert_eq!(result, Some(None));
        let expected_drops = dropped_for.map(|reason| {
            if reason.is_empty() {
                "was kicked.".to_owned()
            } else {
                reason.to_owned()
            }
        });
        assert_eq!(
            fixtures.commands.dropped(),
            expected_drops.into_iter().collect::<Vec<_>>()
        );
    });
}

#[rstest]
#[case("kick")]
#[case("kickban")]
#[case("red")]
#[case("blue")]
#[case("spectate")]
#[case("free")]
#[case("addmod")]
#[case("addadmin")]
#[case("demote")]
#[case("mute")]
#[case("unmute")]
#[case("slap")]
#[case("slay")]
#[case("sound")]
#[case("music")]
#[case("random")]
#[case("switch")]
#[case("map")]
#[case("teamsize")]
#[cfg_attr(miri, ignore)]
#[serial]
fn commands_without_arguments_ask_for_usage(_pyshinqlx_setup: (), #[case] command: &str) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run(command, &fixtures.caller, &[command]);

        assert_eq!(result, Some(Some(USAGE)));
    });
}

#[rstest]
#[case("7")]
#[case("76561198000000042")]
#[cfg_attr(miri, ignore)]
#[serial]
fn player_commands_reject_unknown_ids(_pyshinqlx_setup: (), #[case] ident: &str) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run("kick", &fixtures.caller, &["kick", ident]);

        assert_eq!(result, Some(None));
        assert_eq!(fixtures.replies(), ["Invalid ID."]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn muting_oneself_is_refused(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run("mute", &fixtures.caller, &["mute", "1"]);

        assert_eq!(result, Some(None));
        assert_eq!(fixtures.replies(), ["I refuse."]);
    });
}

#[rstest]
#[case(&["slap", "3"], "slap 3 0")]
#[case(&["slap", "3", "-5"], "slap 3 -5")]
#[case(&["slay", "3"], "slay 3")]
#[cfg_attr(miri, ignore)]
#[serial]
fn slap_and_slay_stop_all_processing(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected: &'static str,
) {
    let engine = engine_with_cvars(&NO_VOTEPASS).with_execute_console_command(expected, 1);
    with_essentials_engine(engine, None, |fixtures| {
        let result = fixtures.run(words[0], &fixtures.caller, words);

        assert_eq!(result, Some(Some(STOP_ALL)));
    });
}

#[rstest]
#[case(&["slap", "7"], "Invalid ID.")]
#[case(&["slay", "7"], "Invalid ID.")]
#[case(&["slap", "3", "lots"], "Invalid damage value.")]
#[cfg_attr(miri, ignore)]
#[serial]
fn slap_and_slay_tell_the_caller_about_invalid_values(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected: &str,
) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run(words[0], &fixtures.caller, words);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), [expected]);
    });
}

#[rstest]
#[case("cmd_shuffle", "forceshuffle")]
#[case("cmd_pause", "pause")]
#[case("cmd_unpause", "unpause")]
#[cfg_attr(miri, ignore)]
#[serial]
fn game_control_commands_send_the_console_command(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] expected: &'static str,
) {
    let engine = engine_with_cvars(&NO_VOTEPASS).with_execute_console_command(expected, 1);
    with_essentials_engine(engine, None, |fixtures| {
        let handler = match command {
            "cmd_shuffle" => Essentials::cmd_shuffle,
            "cmd_pause" => Essentials::cmd_pause,
            _ => Essentials::cmd_unpause,
        };

        let result = handler(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["x"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn sounds_toggles_the_flag_of_the_player(_pyshinqlx_setup: ()) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        let toggle = || {
            Essentials::cmd_enable_sounds(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["sounds"]),
                &fixtures.channel,
            )
        };

        assert_eq!(toggle().ok(), Some(Some(STOP_ALL)));
        assert_eq!(toggle().ok(), Some(Some(STOP_ALL)));

        assert_eq!(
            fixtures.told(&fixtures.caller),
            [
                "Sounds have been disabled. Use ^6!sounds^7 to enable them again.",
                "Sounds have been enabled. Use ^6!sounds^7 to disable them again."
            ]
        );
    });
}

#[rstest]
#[case("sound", "playSound boom")]
#[case("music", "playMusic boom")]
#[cfg_attr(miri, ignore)]
#[serial]
fn sounds_are_played_for_everybody_who_enabled_them(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] played: &str,
) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(fixtures.commands.sent(), [played, played]);
    });
}

#[rstest]
#[case("sound", "playSound boom")]
#[case("music", "playMusic boom")]
#[cfg_attr(miri, ignore)]
#[serial]
fn sounds_skip_players_who_disabled_them(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] played: &str,
) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        fixtures
            .database
            .call_method1("set_flag", (&fixtures.target, SOUNDS_FLAG, false))
            .expect("could not set the flag");

        let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(fixtures.commands.sent(), [played]);
    });
}

#[rstest]
#[case("sound")]
#[case("music")]
#[cfg_attr(miri, ignore)]
#[serial]
fn sounds_are_refused_for_players_who_disabled_them(_pyshinqlx_setup: (), #[case] command: &str) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        fixtures
            .database
            .call_method1("set_flag", (&fixtures.caller, SOUNDS_FLAG, false))
            .expect("could not set the flag");

        let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            ["Sounds are disabled. Use ^6!sounds^7 to enable them again."]
        );
        assert!(
            fixtures
                .commands
                .sent()
                .iter()
                .all(|command| command.starts_with("print"))
        );
    });
}

#[rstest]
#[case("sound", "music/bad")]
#[case("music", "sound/bad")]
#[cfg_attr(miri, ignore)]
#[serial]
fn invalid_sounds_are_reported_only_to_the_caller(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] path: &str,
) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        let result = fixtures.run(command, &fixtures.caller, &[command, path]);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), ["Invalid sound."]);
        assert!(
            fixtures
                .commands
                .sent()
                .iter()
                .all(|command| command.starts_with("print"))
        );
    });
}

#[rstest]
#[case(true, "stopsound", &["clearSounds"])]
#[case(true, "stopmusic", &["stopMusic"])]
#[case(false, "stopsound", &[])]
#[case(false, "stopmusic", &[])]
#[cfg_attr(miri, ignore)]
#[serial]
fn stopping_sounds_respects_the_flag(
    _pyshinqlx_setup: (),
    #[case] enabled: bool,
    #[case] command: &str,
    #[case] expected: &[&str],
) {
    with_essentials(&VOTE_CVARS, None, |fixtures| {
        fixtures
            .database
            .call_method1("set_flag", (&fixtures.caller, SOUNDS_FLAG, enabled))
            .expect("could not set the flag");
        let handler = if command == "stopsound" {
            Essentials::cmd_stopsound
        } else {
            Essentials::cmd_stopmusic
        };

        let result = handler(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&[command]),
            &fixtures.channel,
        );

        assert_eq!(
            result.ok(),
            Some(if enabled { None } else { Some(STOP_ALL) })
        );
        let sent = fixtures.commands.sent();
        let non_print = sent
            .iter()
            .filter(|command| !command.starts_with("print"))
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(non_print, expected);
    });
}

#[rstest]
#[case(true, true, &[])]
#[case(true, false, &[])]
#[case(false, true, &["There is no active vote!"])]
#[cfg_attr(miri, ignore)]
#[serial]
fn yes_and_no_decide_the_active_vote(
    _pyshinqlx_setup: (),
    #[case] active: bool,
    #[case] yes: bool,
    #[case] expected_replies: &[&str],
) {
    let engine = with_vote(engine_with_cvars(&NO_VOTEPASS), active, None);
    with_essentials_engine(engine, None, |fixtures| {
        let handler = if yes {
            Essentials::cmd_yes
        } else {
            Essentials::cmd_no
        };

        let result = handler(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["x"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.replies(), expected_replies);
        assert_eq!(
            fixtures.forced(),
            if active { vec![yes; 2] } else { vec![] }
        );
    });
}

#[rstest]
#[case("0")]
#[case("-3")]
#[case("many")]
#[cfg_attr(miri, ignore)]
#[serial]
fn random_needs_a_positive_limit(_pyshinqlx_setup: (), #[case] limit: &str) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run("random", &fixtures.caller, &["random", limit]);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            ["Invalid upper limit. Use a positive integer."]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn random_announces_a_number_up_to_the_limit(_pyshinqlx_setup: ()) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = fixtures.run("random", &fixtures.caller, &["random", "1"]);

        assert_eq!(result, Some(None));
        assert_eq!(fixtures.replies(), ["^3Random number is: ^51"]);
    });
}

#[rstest]
#[case(&["switch", "1"], Some(USAGE), &[], &[])]
#[case(&["switch", "7", "3"], None, &["The first ID is invalid."], &[])]
#[case(&["switch", "1", "7"], None, &["The second ID is invalid."], &[])]
#[case(&["switch", "1", "3"], None, &[], &["put 1 blue", "put 3 red"])]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_swaps_the_teams_of_two_players(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected_code: Option<i32>,
    #[case] expected_replies: &[&str],
    #[case] expected_commands: &[&'static str],
) {
    let engine = expected_commands
        .iter()
        .fold(engine_with_cvars(&NO_VOTEPASS), |engine, command| {
            engine.with_execute_console_command(*command, 1)
        });
    with_essentials_engine(engine, None, |fixtures| {
        let result = fixtures.run("switch", &fixtures.caller, words);

        assert_eq!(result, Some(expected_code));
        assert_eq!(fixtures.replies(), expected_replies);
    });
}

#[rstest]
#[case("cmd_lock", &["lock"], "lock")]
#[case("cmd_lock", &["lock", "red"], "lock red")]
#[case("cmd_lock", &["lock", "Blue"], "lock blue")]
#[case("cmd_unlock", &["unlock", "s"], "unlock spectator")]
#[case("cmd_unlock", &["unlock"], "unlock")]
#[cfg_attr(miri, ignore)]
#[serial]
fn teams_are_locked_and_unlocked_by_their_initial(
    _pyshinqlx_setup: (),
    #[case] command: &str,
    #[case] words: &[&str],
    #[case] expected: &'static str,
) {
    let engine = engine_with_cvars(&NO_VOTEPASS).with_execute_console_command(expected, 1);
    with_essentials_engine(engine, None, |fixtures| {
        let handler = if command == "cmd_lock" {
            Essentials::cmd_lock
        } else {
            Essentials::cmd_unlock
        };

        let result = handler(
            &fixtures.plugin,
            &fixtures.caller,
            msg(words),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
    });
}

#[rstest]
#[case("x")]
#[case("")]
#[cfg_attr(miri, ignore)]
#[serial]
fn locking_an_unknown_team_is_refused(_pyshinqlx_setup: (), #[case] team: &str) {
    with_essentials(&NO_VOTEPASS, None, |fixtures| {
        let result = Essentials::cmd_lock(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["lock", team]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(Some(STOP_ALL)));
        assert_eq!(fixtures.told(&fixtures.caller), ["Invalid team."]);
    });
}

#[rstest]
#[case(Some("PRE_GAME"), 1, &[])]
#[case(Some("IN_PROGRESS"), 0, &["But the game's already in progress, you silly goose!"])]
#[case(None, 0, &["There is no game running."])]
#[cfg_attr(miri, ignore)]
#[serial]
fn allready_only_works_during_warmup(
    _pyshinqlx_setup: (),
    #[case] state: Option<&str>,
    #[case] commands: usize,
    #[case] expected_replies: &[&str],
) {
    let engine = with_game(
        engine_with_cvars(&NO_VOTEPASS).with_execute_console_command("allready", commands),
        state,
    );
    with_essentials_engine(engine, None, |fixtures| {
        let result = Essentials::cmd_allready(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["allready"]),
            &fixtures.channel,
        );

        assert_eq!(result.as_ref().ok(), Some(&None), "{result:?}");
        assert_eq!(fixtures.replies(), expected_replies);
    });
}

#[rstest]
#[case(Some("IN_PROGRESS"), 1, &[])]
#[case(Some("PRE_GAME"), 0, &["But the game isn't even on, you doofus!"])]
#[case(None, 0, &["There is no game running."])]
#[cfg_attr(miri, ignore)]
#[serial]
fn abort_only_works_during_a_game(
    _pyshinqlx_setup: (),
    #[case] state: Option<&str>,
    #[case] commands: usize,
    #[case] expected_replies: &[&str],
) {
    let engine = with_game(
        engine_with_cvars(&NO_VOTEPASS).with_execute_console_command("map_restart", commands),
        state,
    );
    with_essentials_engine(engine, None, |fixtures| {
        let result = Essentials::cmd_abort(
            &fixtures.plugin,
            &fixtures.caller,
            msg(&["abort"]),
            &fixtures.channel,
        );

        assert_eq!(result.ok(), Some(None));
        assert_eq!(fixtures.replies(), expected_replies);
    });
}

#[rstest]
#[case(&["map", "campgrounds"], "map campgrounds")]
#[case(&["map", "campgrounds", "ffa"], "map campgrounds ffa")]
#[cfg_attr(miri, ignore)]
#[serial]
fn map_changes_the_map_with_an_optional_factory(
    _pyshinqlx_setup: (),
    #[case] words: &[&str],
    #[case] expected: &'static str,
) {
    let engine = engine_with_cvars(&NO_VOTEPASS).with_execute_console_command(expected, 1);
    with_essentials_engine(engine, None, |fixtures| {
        let result = fixtures.run("map", &fixtures.caller, words);

        assert_eq!(result, Some(None));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn teamsize_sets_the_size_and_announces_it(_pyshinqlx_setup: ()) {
    let engine = with_game(
        engine_with_cvars(&[("qlx_commandPrefix", c"!"), ("teamsize", c"4")]),
        Some("PRE_GAME"),
    )
    .configure(|engine| {
        engine
            .expect_set_cvar_forced()
            .with(
                predicate::eq("teamsize"),
                predicate::eq("6"),
                predicate::eq(false),
            )
            .times(1)
            .returning(|_, _, _| None);
    });
    with_essentials_engine(engine, None, |fixtures| {
        let py = fixtures.plugin.py();
        let chat_channel = Py::new(
            py,
            TeamChatChannel::py_new(
                py,
                "all",
                "chat",
                "print \"{}\n\"\n",
                py.None().bind(py),
                None,
            ),
        )
        .expect("could not create the chat channel");
        CHAT_CHANNEL.store(Some(chat_channel.into()));

        let result = fixtures.run("teamsize", &fixtures.caller, &["ts", "6"]);
        CHAT_CHANNEL.store(None);

        assert_eq!(result, Some(Some(STOP_ALL)));
        assert_eq!(
            fixtures.told(&fixtures.caller),
            ["The teamsize has been set to ^66^7 by Caller^7."]
        );
    });
}

#[rstest]
#[case(Some("PRE_GAME"), "^7Unintelligible size.", "x")]
#[case(None, "There is no game running.", "4")]
#[cfg_attr(miri, ignore)]
#[serial]
fn teamsize_reports_problems(
    _pyshinqlx_setup: (),
    #[case] state: Option<&str>,
    #[case] expected: &str,
    #[case] size: &str,
) {
    let engine = with_game(engine_with_cvars(&NO_VOTEPASS), state);
    with_essentials_engine(engine, None, |fixtures| {
        let result = fixtures.run("teamsize", &fixtures.caller, &["ts", size]);

        assert_eq!(result, Some(None));
        assert_eq!(fixtures.replies(), [expected]);
    });
}
