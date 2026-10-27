use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::{exceptions::PyValueError, intern, prelude::*, types::PyDict};

use super::{
    register_command, register_hook,
    target::{INVALID_CLIENT_ID, Resolution, display_name, reply, resolve_text, stop_all, usage},
    tell,
    timed_entries::{
        Entry, active_entries, latest_entry, length_in_seconds, now, revoke_entries, store_entry,
    },
};
use crate::ffi::python::{
    Plugin, PythonReturnCodes, commands::CommandPriorities, next_frame, plugin::PluginMethods,
};

const STOP_ALL: i32 = PythonReturnCodes::RET_STOP_ALL as i32;
const MINIMUM_PERMISSION_TO_BE_PROTECTED: i32 = 2;

/// Mutes players temporarily, also across server restarts.
#[pyclass(name = "silence", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Silence {
    silenced: Mutex<HashMap<i64, Silenced>>,
}

type Silenced = Entry;

impl Silenced {
    fn notice(&self) -> String {
        if self.reason.is_empty() {
            format!("You are muted on this server until ^6{}^7.", self.expires)
        } else {
            format!(
                "You are muted on this server until ^6{}^7: {}",
                self.expires, self.reason
            )
        }
    }
}

fn silences_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}:silences")
}

fn mute(target_player: &Bound<'_, PyAny>) -> PyResult<()> {
    match target_player.call_method0(intern!(target_player.py(), "mute")) {
        Err(err) if err.is_instance_of::<PyValueError>(target_player.py()) => Ok(()),
        result => result.map(|_| ()),
    }
}

#[pymethods]
impl Silence {
    fn handle_player_loaded(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        let steam_id = player.getattr(intern!(slf.py(), "steam_id"))?.extract()?;
        let Some(silenced) = latest_entry(&slf.as_super().get_db()?, &silences_key(steam_id))?
        else {
            return Ok(());
        };

        let notice = silenced.notice();
        slf.get().silenced.lock().insert(steam_id, silenced);
        player.call_method0(intern!(slf.py(), "mute"))?;
        tell(player, &notice)
    }

    fn handle_player_disconnect(
        &self,
        player: &Bound<'_, PyAny>,
        _reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let steam_id = player
            .getattr(intern!(player.py(), "steam_id"))?
            .extract()?;
        self.silenced.lock().remove(&steam_id);
        Ok(())
    }

    fn handle_client_command(
        slf: &Bound<'_, Self>,
        player: Option<&Bound<'_, PyAny>>,
        cmd: &str,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let Some(player) = player else {
            return Ok(None);
        };
        let steam_id = player.getattr(intern!(py, "steam_id"))?.extract()?;
        let Some(silenced) = slf.get().silenced.lock().get(&steam_id).cloned() else {
            return Ok(None);
        };

        let command = cmd.to_lowercase();
        if !(command.starts_with("say ") || command.starts_with("say_team ")) {
            return Ok(None);
        }

        if now() < silenced.score {
            tell(player, &silenced.notice())?;
        } else {
            slf.get().silenced.lock().remove(&steam_id);
            player.call_method0(intern!(py, "unmute"))?;
            let client_command = py
                .import(intern!(py, "shinqlx"))?
                .getattr(intern!(py, "client_command"))?;
            next_frame(&client_command)?.call1((player.getattr(intern!(py, "id"))?, cmd))?;
        }
        Ok(Some(STOP_ALL))
    }

    fn handle_userinfo<'py>(
        slf: &Bound<'py, Self>,
        player: &Bound<'py, PyAny>,
        changed: &Bound<'py, PyDict>,
    ) -> PyResult<Option<Bound<'py, PyDict>>> {
        let py = slf.py();
        let steam_id = player.getattr(intern!(py, "steam_id"))?.extract()?;
        if !slf.get().silenced.lock().contains_key(&steam_id)
            || !changed.contains(intern!(py, "name"))?
        {
            return Ok(None);
        }

        let current_name = player.getattr(intern!(py, "name"))?.extract::<String>()?;
        changed.set_item(
            intern!(py, "name"),
            current_name
                .strip_suffix("^7")
                .unwrap_or(current_name.as_str()),
        )?;
        Ok(Some(changed.clone()))
    }

    fn cmd_silence(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 4 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;
        let steam_id = target.steam_id();
        let database = slf.as_super().get_db()?;

        if database
            .call_method1(
                intern!(py, "has_permission"),
                (steam_id, MINIMUM_PERMISSION_TO_BE_PROTECTED),
            )?
            .is_truthy()?
        {
            return reply(
                channel,
                &format!(
                    "^6{name}^7 has permission level {MINIMUM_PERMISSION_TO_BE_PROTECTED} or more and cannot be silenced."
                ),
            );
        }

        let Some(seconds) = length_in_seconds(&format!("{} {}", msg[2], msg[3])) else {
            return usage();
        };

        let silenced = store_entry(
            &database,
            &silences_key(steam_id),
            &player.getattr(intern!(py, "steam_id"))?,
            seconds,
            &msg[4..].join(" "),
        )?;

        let expires = silenced.expires.clone();
        if let Some(target_player) = target.player() {
            slf.get().silenced.lock().insert(steam_id, silenced);
            mute(target_player)?;
        }
        reply(
            channel,
            &format!("^6{name} ^7has been silenced. Silence expires on ^6{expires}^7."),
        )
    }

    fn cmd_unsilence(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;
        let steam_id = target.steam_id();

        let database = slf.as_super().get_db()?;
        let key = silences_key(steam_id);
        let silences = active_entries(&database, &key)?;
        if silences.is_empty() {
            return reply(channel, &format!("No active silences on ^6{name}^7 found."));
        }
        revoke_entries(&database, &key, &silences)?;

        slf.get().silenced.lock().remove(&steam_id);
        if let Some(target_player) = target.player() {
            target_player.call_method0(intern!(py, "unmute"))?;
        }
        reply(channel, &format!("^6{name}^7 has been unsilenced."))
    }

    fn cmd_checksilence(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;

        let key = silences_key(target.steam_id());
        let message = match latest_entry(&slf.as_super().get_db()?, &key)? {
            Some(silenced) if silenced.reason.is_empty() => {
                format!("^6{name}^7 is silenced until ^6{}^7.", silenced.expires)
            }
            Some(silenced) => format!(
                "^6{name}^7 is silenced until ^6{}^7 for the following reason:^6 {}",
                silenced.expires, silenced.reason
            ),
            None => format!("^6{name} ^7is not silenced."),
        };
        reply(channel, &message)
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Silence {
            silenced: Mutex::new(HashMap::new()),
        }),
    )?;

    for (event, handler, priority) in [
        (
            "player_loaded",
            "handle_player_loaded",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "player_disconnect",
            "handle_player_disconnect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "client_command",
            "handle_client_command",
            CommandPriorities::PRI_HIGH,
        ),
        ("userinfo", "handle_userinfo", CommandPriorities::PRI_HIGH),
    ] {
        register_hook(plugin.as_super(), event, handler, priority)?;
    }
    for (name, handler, permission, usage) in [
        (
            "silence",
            "cmd_silence",
            2,
            "<id> <length> seconds|minutes|hours|days|... [reason]",
        ),
        ("unsilence", "cmd_unsilence", 2, "<id>"),
        ("checksilence", "cmd_checksilence", 0, "<id>"),
    ] {
        register_command(plugin.as_super(), &[name], handler, permission, usage, None)?;
    }

    Ok(plugin.into_any())
}

#[cfg(test)]
mod silence_tests {
    use core::ffi::CStr;
    use std::collections::HashMap;

    use pyo3::{
        prelude::*,
        types::{IntoPyDict, PyDict, PyDictMethods},
    };
    use rstest::rstest;

    use super::{STOP_ALL, Silence, Silenced, silences_key};
    use crate::{
        ffi::python::{
            Plugin, PythonReturnCodes,
            native_plugins::{
                test_support::{FakeRedis, REDIS_2, REDIS_5, fixture_module},
                timed_entries::now,
            },
            plugin::PluginMethods,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, with_connected_players},
        },
        prelude::*,
    };

    const CALLER_ID: i64 = 76_561_198_000_000_001;
    const TARGET_ID: i64 = 76_561_198_000_000_003;
    const FAR_FUTURE: f64 = 4_000_000_000.0;
    const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;

    const FIXTURES: &CStr = c"
class Player:
    def __init__(self, client_id, steam_id, name):
        self.id = client_id
        self.steam_id = steam_id
        self.name = name
        self.told = []
        self.muted = False
        self.mute_raises = False

    def tell(self, message):
        self.told.append(message)

    def mute(self):
        if self.mute_raises:
            raise ValueError('cannot mute')
        self.muted = True

    def unmute(self):
        self.muted = False


caller = Player(1, 76561198000000001, 'Caller^7')
target = Player(3, 76561198000000003, 'Eve^7')


def find_player(cls, client_id, player_list=None):
    if client_id == 3:
        return target
    if client_id == 1:
        return caller
    raise shinqlx.NonexistentPlayerError(client_id)


player = classmethod(find_player)
";

    struct Fixtures<'py> {
        plugin: Bound<'py, Silence>,
        database: Bound<'py, PyAny>,
        channel: Bound<'py, PyAny>,
        caller: Bound<'py, PyAny>,
        target: Bound<'py, PyAny>,
    }

    impl Fixtures<'_> {
        fn replies(&self) -> Vec<String> {
            self.channel
                .getattr("replies")
                .and_then(|replies| replies.extract())
                .expect("could not get the replies")
        }

        fn told(&self, player: &Bound<'_, PyAny>) -> Vec<String> {
            player
                .getattr("told")
                .and_then(|told| told.extract())
                .expect("could not get told messages")
        }

        fn muted(&self) -> bool {
            self.target
                .getattr("muted")
                .and_then(|muted| muted.extract())
                .expect("could not get the muted flag")
        }

        fn zsets(&self) -> HashMap<String, HashMap<String, f64>> {
            self.database
                .getattr("zsets")
                .and_then(|zsets| zsets.extract())
                .expect("could not get the sorted sets")
        }

        fn hashes(&self) -> HashMap<String, HashMap<String, String>> {
            self.database
                .getattr("hashes")
                .and_then(|hashes| hashes.extract())
                .expect("could not get the hashes")
        }

        fn silence(&self, steam_id: i64, silence_id: u32, score: f64, expires: &str, reason: &str) {
            let key = silences_key(steam_id);
            self.database
                .getattr("zsets")
                .and_then(|zsets| {
                    zsets
                        .call_method1("setdefault", (&key, PyDict::new(zsets.py())))?
                        .set_item(silence_id.to_string(), score)
                })
                .expect("could not store the score");
            self.database
                .getattr("hashes")
                .and_then(|hashes| {
                    hashes.set_item(
                        format!("{key}:{silence_id}"),
                        [("expires", expires), ("reason", reason)].into_py_dict(hashes.py())?,
                    )
                })
                .expect("could not store the details");
        }

        fn remember(&self, steam_id: i64, score: f64, reason: &str) {
            self.plugin.get().silenced.lock().insert(
                steam_id,
                Silenced {
                    expires: "2096-10-02 12:00:00".to_owned(),
                    score,
                    reason: reason.to_owned(),
                },
            );
        }

        fn remembered(&self, steam_id: i64) -> Option<Silenced> {
            self.plugin.get().silenced.lock().get(&steam_id).cloned()
        }

        fn grant(&self, steam_id: i64, level: i32) {
            self.database
                .getattr("permissions")
                .and_then(|permissions| permissions.set_item(steam_id, level))
                .expect("could not grant the permission");
        }
    }

    fn with_silence<F>(redis: &'static CStr, players: &[(i32, &'static str)], test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_connected_players(MockEngineBuilder::default(), players, || {
            Python::attach(|py| {
                let _fake_redis = FakeRedis::install(py, redis);
                let module = fixture_module(py, FIXTURES);
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                let _restore_player = RestoredAttribute::capture(plugin_class.as_any(), "player");
                if players.is_empty() {
                    plugin_class
                        .setattr("player", module.getattr("player").expect("no player"))
                        .expect("could not replace Plugin.player");
                }

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Silence {
                        silenced: Default::default(),
                    }),
                )
                .expect("could not create the plugin");
                let database = plugin.as_super().get_db().expect("no database");
                let get = |name: &str| module.getattr(name).expect("missing fixture");
                test(Fixtures {
                    plugin,
                    database,
                    channel: get("Channel").call0().expect("no channel"),
                    caller: get("caller"),
                    target: get("target"),
                });
            });
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[rstest]
    #[case("", "You are muted on this server until ^6soon^7.")]
    #[case("rude", "You are muted on this server until ^6soon^7: rude")]
    fn the_notice_names_the_expiry_and_the_reason(#[case] reason: &str, #[case] expected: &str) {
        let silenced = Silenced {
            expires: "soon".to_owned(),
            score: 1.0,
            reason: reason.to_owned(),
        };

        assert_eq!(silenced.notice(), expected);
    }

    #[rstest]
    #[case(&["silence"])]
    #[case(&["silence", "3"])]
    #[case(&["silence", "3", "1"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_without_enough_arguments_asks_for_usage(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[case(&["silence", "nobody", "1", "day"])]
    #[case(&["unsilence", "nobody"])]
    #[case(&["checksilence", "nobody"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn commands_stop_all_processing_for_a_name_that_matches_nobody(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            let command = match words[0] {
                "silence" => Silence::cmd_silence,
                "unsilence" => Silence::cmd_unsilence,
                _ => Silence::cmd_checksilence,
            };

            let result = command(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_of_an_unknown_client_id_replies(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["silence", "9", "1", "day"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["Invalid client ID. Use either a client ID or a SteamID64."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_does_not_touch_players_with_permission_level_two(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.grant(TARGET_ID, 2);

            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["silence", "3", "1", "day"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["^6Eve^7 has permission level 2 or more and cannot be silenced."]
            );
            assert!(fixtures.zsets().is_empty());
            assert!(!fixtures.muted());
        });
    }

    #[rstest]
    #[case(&["silence", "3", "0", "days"])]
    #[case(&["silence", "3", "soon", "days"])]
    #[case(&["silence", "3", "1", "fortnight"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_with_an_invalid_length_asks_for_usage(_pyshinqlx_setup: (), #[case] words: &[&str]) {
        with_silence(REDIS_5, &[], |fixtures| {
            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert!(fixtures.zsets().is_empty());
            assert!(!fixtures.muted());
        });
    }

    #[rstest]
    #[case(REDIS_5, true)]
    #[case(REDIS_2, true)]
    #[case(REDIS_5, false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_stores_the_silence_and_mutes_a_connected_player(
        _pyshinqlx_setup: (),
        #[case] redis: &'static CStr,
        #[case] hset_supports_mapping: bool,
    ) {
        with_silence(redis, &[], |fixtures| {
            fixtures
                .database
                .setattr("hset_supports_mapping", hset_supports_mapping)
                .expect("could not configure the database");
            let before = now();

            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["silence", "3", "1", "day", "very", "rude"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let key = silences_key(TARGET_ID);
            let score = fixtures.zsets()[&key]["0"];
            assert!((before + 86_400.0..=now() + 86_400.0).contains(&score));
            let details = &fixtures.hashes()[&format!("{key}:0")];
            assert_eq!(details["reason"], "very rude");
            assert_eq!(details["issued_by"], CALLER_ID.to_string());
            assert!(fixtures.muted());
            assert_eq!(
                fixtures
                    .remembered(TARGET_ID)
                    .map(|silenced| silenced.reason),
                Some("very rude".to_owned())
            );
            let replies = fixtures.replies();
            assert_eq!(replies.len(), 1);
            assert!(replies[0].starts_with("^6Eve ^7has been silenced. Silence expires on ^6"));
            assert!(replies[0].ends_with("^7."));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_of_an_offline_steam_id_only_stores_the_silence(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.silence(76_561_198_000_000_099, 0, FAR_FUTURE, "x", "");

            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["silence", "76561198000000099", "2", "weeks"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let key = silences_key(76_561_198_000_000_099);
            assert_eq!(fixtures.zsets()[&key].len(), 2);
            assert!(fixtures.hashes().contains_key(&format!("{key}:1")));
            assert!(fixtures.remembered(76_561_198_000_000_099).is_none());
            assert!(fixtures.replies()[0].starts_with("^676561198000000099 ^7has been silenced."));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silence_ignores_players_that_cannot_be_muted(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures
                .target
                .setattr("mute_raises", true)
                .expect("could not configure the player");

            let result = Silence::cmd_silence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["silence", "3", "1", "hour"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert!(fixtures.remembered(TARGET_ID).is_some());
        });
    }

    #[rstest]
    #[case(&["unsilence"])]
    #[case(&["checksilence"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unsilence_and_checksilence_without_a_target_ask_for_usage(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            for command in [Silence::cmd_unsilence, Silence::cmd_checksilence] {
                let result = command(
                    &fixtures.plugin,
                    &fixtures.caller,
                    msg(words),
                    &fixtures.channel,
                );

                assert_eq!(result.ok(), Some(Some(USAGE)));
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unsilence_without_an_active_silence_says_so(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.silence(TARGET_ID, 0, 1.0, "2000-01-01 00:00:00", "old");

            let result = Silence::cmd_unsilence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["unsilence", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["No active silences on ^6Eve^7 found."]);
        });
    }

    #[rstest]
    #[case(REDIS_5)]
    #[case(REDIS_2)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unsilence_lifts_every_active_silence(_pyshinqlx_setup: (), #[case] redis: &'static CStr) {
        with_silence(redis, &[], |fixtures| {
            fixtures.silence(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", "one");
            fixtures.silence(
                TARGET_ID,
                1,
                FAR_FUTURE + 10.0,
                "2096-10-02 12:00:10",
                "two",
            );
            fixtures.remember(TARGET_ID, FAR_FUTURE, "one");
            fixtures
                .target
                .setattr("muted", true)
                .expect("could not mute the player");

            let result = Silence::cmd_unsilence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["unsilence", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let scores = &fixtures.zsets()[&silences_key(TARGET_ID)];
            assert_eq!(scores["0"], 0.0);
            assert_eq!(scores["1"], 0.0);
            assert!(fixtures.remembered(TARGET_ID).is_none());
            assert!(!fixtures.muted());
            assert_eq!(fixtures.replies(), ["^6Eve^7 has been unsilenced."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checksilence_reports_the_state_of_a_target(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            for (target, expected) in [
                ("3", "^6Eve ^7is not silenced."),
                (
                    "76561198000000050",
                    "^676561198000000050^7 is silenced until ^62096-10-02 12:00:00^7.",
                ),
                (
                    "76561198000000051",
                    "^676561198000000051^7 is silenced until ^62096-10-02 12:00:00^7 for the following reason:^6 spam",
                ),
            ] {
                if target.ends_with("50") {
                    fixtures.silence(
                        76_561_198_000_000_050,
                        0,
                        FAR_FUTURE,
                        "2096-10-02 12:00:00",
                        "",
                    );
                } else if target.ends_with("51") {
                    fixtures.silence(
                        76_561_198_000_000_051,
                        0,
                        FAR_FUTURE,
                        "2096-10-02 12:00:00",
                        "spam",
                    );
                }

                let result = Silence::cmd_checksilence(
                    &fixtures.plugin,
                    &fixtures.caller,
                    msg(&["checksilence", target]),
                    &fixtures.channel,
                );

                assert_eq!(result.ok(), Some(None));
                assert_eq!(
                    fixtures.replies().last().map(String::as_str),
                    Some(expected)
                );
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checksilence_finds_a_player_by_name(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[(2, "Bob")], |fixtures| {
            fixtures.silence(1002, 0, FAR_FUTURE, "2096-10-02 12:00:00", "");

            let result = Silence::cmd_checksilence(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["checksilence", "bob"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["^6Bob^7 is silenced until ^62096-10-02 12:00:00^7."]
            );
        });
    }

    #[rstest]
    #[case("", "You are muted on this server until ^62096-10-02 12:00:00^7.")]
    #[case(
        "spam",
        "You are muted on this server until ^62096-10-02 12:00:00^7: spam"
    )]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn loading_a_silenced_player_mutes_and_informs_them(
        _pyshinqlx_setup: (),
        #[case] reason: &str,
        #[case] expected: &str,
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.silence(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", reason);

            let result = Silence::handle_player_loaded(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert!(fixtures.muted());
            assert_eq!(fixtures.told(&fixtures.target), [expected]);
            assert_eq!(
                fixtures
                    .remembered(TARGET_ID)
                    .map(|silenced| silenced.score),
                Some(FAR_FUTURE)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn loading_a_player_without_a_silence_changes_nothing(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            let result = Silence::handle_player_loaded(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert!(!fixtures.muted());
            assert!(fixtures.told(&fixtures.target).is_empty());
            assert!(fixtures.remembered(TARGET_ID).is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn disconnecting_forgets_the_silence(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.remember(TARGET_ID, FAR_FUTURE, "");

            let result = fixtures
                .plugin
                .get()
                .handle_player_disconnect(&fixtures.target, &fixtures.channel);

            assert!(result.is_ok());
            assert!(fixtures.remembered(TARGET_ID).is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_commands_of_unknown_or_unsilenced_players_pass(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            assert!(
                Silence::handle_client_command(&fixtures.plugin, None, "say hi")
                    .is_ok_and(|result| result.is_none())
            );
            assert!(
                Silence::handle_client_command(&fixtures.plugin, Some(&fixtures.target), "say hi")
                    .is_ok_and(|result| result.is_none())
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silenced_players_may_use_other_client_commands(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.remember(TARGET_ID, FAR_FUTURE, "");

            let result = Silence::handle_client_command(
                &fixtures.plugin,
                Some(&fixtures.target),
                "team red",
            );

            assert!(result.is_ok_and(|result| result.is_none()));
            assert!(fixtures.told(&fixtures.target).is_empty());
        });
    }

    #[rstest]
    #[case("say hello")]
    #[case("SAY hello")]
    #[case("say_team hello")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silenced_players_cannot_chat(_pyshinqlx_setup: (), #[case] command: &str) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.remember(TARGET_ID, FAR_FUTURE, "spam");

            let result =
                Silence::handle_client_command(&fixtures.plugin, Some(&fixtures.target), command);

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.target),
                ["You are muted on this server until ^62096-10-02 12:00:00^7: spam"]
            );
            assert!(fixtures.remembered(TARGET_ID).is_some());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_expired_silence_is_lifted_and_the_chat_message_repeated_in_the_next_frame(
        _pyshinqlx_setup: (),
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.remember(TARGET_ID, 1.0, "");
            fixtures
                .target
                .setattr("muted", true)
                .expect("could not mute the player");

            let result = Silence::handle_client_command(
                &fixtures.plugin,
                Some(&fixtures.target),
                "say hello",
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert!(fixtures.remembered(TARGET_ID).is_none());
            assert!(!fixtures.muted());
            assert!(fixtures.told(&fixtures.target).is_empty());

            Python::attach(|py| {
                let queued = py
                    .import("shinqlx")
                    .and_then(|shinqlx| shinqlx.getattr("next_frame_tasks"))
                    .and_then(|tasks| tasks.call_method0("get_nowait"))
                    .expect("the chat message should have been repeated");
                let (function, args, _kwargs) = queued
                    .extract::<(Bound<'_, PyAny>, (i32, String), Bound<'_, PyAny>)>()
                    .expect("unexpected task");
                assert!(
                    py.import("shinqlx")
                        .and_then(|shinqlx| shinqlx.getattr("client_command"))
                        .is_ok_and(|client_command| function.is(&client_command))
                );
                assert_eq!(args, (3, "say hello".to_owned()));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn silenced_players_cannot_change_their_name(_pyshinqlx_setup: ()) {
        with_silence(REDIS_5, &[], |fixtures| {
            fixtures.remember(TARGET_ID, FAR_FUTURE, "");
            Python::attach(|py| {
                let changed = PyDict::new(py);
                changed.set_item("name", "Mallory").expect("no name");

                let result = Silence::handle_userinfo(&fixtures.plugin, &fixtures.target, &changed);

                let returned = result
                    .expect("the handler failed")
                    .expect("the userinfo should have been changed");
                assert!(
                    returned
                        .get_item("name")
                        .is_ok_and(|name| name.is_some_and(|name| name.to_string() == "Eve"))
                );
            });
        });
    }

    #[rstest]
    #[case(true, false)]
    #[case(false, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn other_userinfo_changes_pass(
        _pyshinqlx_setup: (),
        #[case] silenced: bool,
        #[case] with_name: bool,
    ) {
        with_silence(REDIS_5, &[], |fixtures| {
            if silenced {
                fixtures.remember(TARGET_ID, FAR_FUTURE, "");
            }
            Python::attach(|py| {
                let changed = PyDict::new(py);
                if with_name {
                    changed.set_item("name", "Mallory").expect("no name");
                } else {
                    changed.set_item("model", "sarge").expect("no model");
                }

                let result = Silence::handle_userinfo(&fixtures.plugin, &fixtures.target, &changed);

                assert!(result.is_ok_and(|result| result.is_none()));
            });
        });
    }
}

#[cfg(test)]
mod silence_creation_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::python::{
            COMMANDS, EVENT_DISPATCHERS, Plugin,
            commands::CommandInvoker,
            events::{
                ClientCommandDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                PlayerDisconnectDispatcher, PlayerLoadedDispatcher, UserinfoDispatcher,
            },
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, engine_with_cvars},
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_registers_the_hooks_and_the_commands(_pyshinqlx_setup: ()) {
        let mut engine = engine_with_cvars(&[("zmq_stats_enable", c"0")]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        pass
",
                    c"",
                    c"",
                )
                .expect("could not create the fixtures");
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                let dispatchers = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                for dispatcher in [
                    py.get_type::<PlayerLoadedDispatcher>(),
                    py.get_type::<PlayerDisconnectDispatcher>(),
                    py.get_type::<ClientCommandDispatcher>(),
                    py.get_type::<UserinfoDispatcher>(),
                ] {
                    dispatchers
                        .add_dispatcher(&dispatcher)
                        .expect("could not add the dispatcher");
                }
                EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));
                COMMANDS.store(Some(
                    Py::new(py, CommandInvoker::py_new())
                        .expect("could not create the command invoker")
                        .into(),
                ));

                let plugin = create(py).expect("could not create the plugin");

                let count = |attribute: &str| {
                    plugin
                        .getattr(attribute)
                        .and_then(|items| items.len())
                        .expect("could not count")
                };
                assert_eq!(count("hooks"), 4);
                assert_eq!(count("commands"), 3);
                assert!(plugin.hasattr(intern!(py, "cmd_silence")).unwrap_or(false));
            });
        });
    }
}
