use pyo3::{exceptions::PyKeyError, intern, prelude::*, types::PyTuple};

use super::{register_command, register_hook};
use crate::ffi::python::{
    Plugin, PythonReturnCodes, commands::CommandPriorities, delay, get_cvar, plugin::PluginMethods,
};

const MOTD_SET_KEY: &str = "minqlx:motd";
const SOUNDS_ENABLED_FLAG: &str = "essentials:sounds_enabled";
const STOP_EVENT: i32 = PythonReturnCodes::RET_STOP_EVENT as i32;
const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;
const LINE_BREAK: &str = "\\n";

/// Greets the players who joined with the message of the day of this server.
#[pyclass(name = "motd", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Motd {
    #[pyo3(get)]
    home: String,
    #[pyo3(get)]
    motd_key: String,
}

fn motd_key_of(path: &str) -> String {
    format!("{MOTD_SET_KEY}:{path}")
}

fn joined_arguments(msg: &[String]) -> String {
    msg.get(1..).unwrap_or_default().join(" ")
}

fn appended(existing: &str, addition: &str) -> String {
    let leading_space = if existing.len() > 2 && existing.ends_with(LINE_BREAK) {
        ""
    } else {
        " "
    };
    format!("{existing}{leading_space}{addition}")
}

fn tell(player: &Bound<'_, PyAny>, message: &str) -> PyResult<()> {
    player
        .call_method1(intern!(player.py(), "tell"), (message,))
        .map(|_| ())
}

fn registered_paths(database: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    database
        .call_method1(intern!(database.py(), "smembers"), (MOTD_SET_KEY,))?
        .try_iter()?
        .map(|path| path?.extract())
        .collect()
}

fn stored_motd(database: &Bound<'_, PyAny>, key: &str) -> PyResult<Option<String>> {
    match database.get_item(key) {
        Ok(motd) => motd.extract().map(Some),
        Err(err) if err.is_instance_of::<PyKeyError>(database.py()) => Ok(None),
        Err(err) => Err(err),
    }
}

fn send_motd_lines(player: &Bound<'_, PyAny>, motd: &str) -> PyResult<()> {
    let header = get_cvar("qlx_motdHeader")?.unwrap_or_default();
    header
        .split(LINE_BREAK)
        .chain(motd.split(LINE_BREAK))
        .try_for_each(|line| tell(player, line))
}

#[pymethods]
impl Motd {
    fn handle_player_loaded(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        delay(py, 2.0)?
            .call1((slf.getattr(intern!(py, "deliver_motd"))?,))?
            .call1((player,))
            .map(|_| ())
    }

    fn deliver_motd(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let Some(motd) = stored_motd(&database, &slf.get().motd_key)? else {
            return Ok(());
        };

        let welcome_sound = get_cvar("qlx_motdSound")?
            .filter(|sound| sound != "0")
            .unwrap_or_default();
        if !welcome_sound.is_empty()
            && database
                .call_method1(intern!(py, "get_flag"), (player, SOUNDS_ENABLED_FLAG, true))?
                .is_truthy()?
        {
            py.get_type::<Plugin>()
                .call_method1(intern!(py, "play_sound"), (welcome_sound, player))?;
        }

        send_motd_lines(player, &motd)
    }

    fn send_motd(_slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>, motd: &str) -> PyResult<()> {
        send_motd_lines(player, motd)
    }

    fn cmd_setmotd(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return Ok(Some(USAGE));
        }
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let motd = slf.get();
        database.call_method1(intern!(py, "sadd"), (MOTD_SET_KEY, &motd.home))?;
        database.set_item(&motd.motd_key, joined_arguments(&msg))?;

        tell(player, "The MOTD has been set.")?;
        Ok(Some(STOP_EVENT))
    }

    fn cmd_setmotdall(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let new_motd = joined_arguments(&msg);
        let pipeline = database.call_method0(intern!(py, "pipeline"))?;
        for path in registered_paths(&database)? {
            pipeline.call_method1(intern!(py, "set"), (motd_key_of(&path), &new_motd))?;
        }
        pipeline.call_method0(intern!(py, "execute"))?;

        tell(player, "All MOTDs have been set.")?;
        Ok(Some(STOP_EVENT))
    }

    fn cmd_getmotd(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let database = slf.as_super().get_db()?;
        match stored_motd(&database, &slf.get().motd_key)? {
            Some(motd) => send_motd_lines(player, &motd)?,
            None => tell(player, "No MOTD has been set.")?,
        }
        Ok(Some(STOP_EVENT))
    }

    fn cmd_clearmotd(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let database = slf.as_super().get_db()?;
        let key = &slf.get().motd_key;
        if database.contains(key)? {
            database.del_item(key)?;
            tell(player, "The MOTD has been cleared.")?;
        } else {
            tell(player, "No MOTD has been set.")?;
        }
        Ok(Some(STOP_EVENT))
    }

    fn cmd_clearmotdall(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let keys = registered_paths(&database)?
            .iter()
            .map(|path| motd_key_of(path))
            .collect::<Vec<_>>();
        if !keys.is_empty() {
            database.call_method1(intern!(py, "delete"), PyTuple::new(py, keys)?)?;
        }

        tell(player, "All MOTDs have been cleared.")?;
        Ok(Some(STOP_EVENT))
    }

    fn cmd_addmotd(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let database = slf.as_super().get_db()?;
        let key = &slf.get().motd_key;
        let addition = joined_arguments(&msg);
        match stored_motd(&database, key)?.filter(|motd| !motd.is_empty()) {
            None => {
                database.set_item(key, addition)?;
                tell(player, "No MOTD was set, so a new one was made.")?;
            }
            Some(motd) => {
                database.set_item(key, appended(&motd, &addition))?;
                tell(player, "The MOTD has been updated.")?;
            }
        }
        Ok(Some(STOP_EVENT))
    }

    fn cmd_addmotdall(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let database = slf.as_super().get_db()?;
        let addition = joined_arguments(&msg);
        for path in registered_paths(&database)? {
            let key = motd_key_of(&path);
            let updated = match stored_motd(&database, &key)? {
                None => addition.clone(),
                Some(motd) => appended(&motd, &addition),
            };
            database.set_item(&key, updated)?;
        }

        tell(player, "Added to all MOTDs.")?;
        Ok(Some(STOP_EVENT))
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let home = get_cvar("fs_homepath")?.ok_or_else(|| {
        pyo3::exceptions::PyEnvironmentError::new_err("cvar fs_homepath is not set")
    })?;
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Motd {
            motd_key: motd_key_of(&home),
            home,
        }),
    )?;

    register_hook(
        plugin.as_super(),
        "player_loaded",
        "handle_player_loaded",
        CommandPriorities::PRI_LOWEST,
    )?;
    for (names, handler, permission, usage) in [
        (&["setmotd", "newmotd"][..], "cmd_setmotd", 4, "<motd>"),
        (
            &["setmotdall", "newmotdall"][..],
            "cmd_setmotdall",
            4,
            "<motd>",
        ),
        (&["getmotd", "motd"][..], "cmd_getmotd", 0, ""),
        (
            &["clearmotd", "removemotd", "remmmotd"][..],
            "cmd_clearmotd",
            4,
            "",
        ),
        (
            &["clearmotdall", "removemotdall", "remmmotdall"][..],
            "cmd_clearmotdall",
            4,
            "",
        ),
        (&["addmotd"][..], "cmd_addmotd", 4, "<more_motd>"),
        (&["addmotdall"][..], "cmd_addmotdall", 4, "<more_motd>"),
    ] {
        register_command(plugin.as_super(), names, handler, permission, usage, None)?;
    }

    let motd = plugin.get();
    plugin
        .as_super()
        .get_db()?
        .call_method1(intern!(py, "sadd"), (MOTD_SET_KEY, &motd.home))?;
    let plugin_class = py.get_type::<Plugin>();
    plugin_class.call_method1(
        intern!(py, "set_cvar_once"),
        ("qlx_motdSound", "sound/vo/crash_new/37b_07_alt.wav"),
    )?;
    plugin_class.call_method1(
        intern!(py, "set_cvar_once"),
        (
            "qlx_motdHeader",
            "^6======= ^7Message of the Day ^6=======^7",
        ),
    )?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod motd_tests {
    use core::{borrow::BorrowMut, ffi::CStr};

    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::{Motd, STOP_EVENT, USAGE};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                Plugin, plugin::PluginMethods, pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::RestoredAttribute,
            },
        },
        prelude::*,
    };

    const HOME: &str = "/ql";
    const KEY: &str = "minqlx:motd:/ql";

    struct Fixtures<'py> {
        plugin: Bound<'py, Motd>,
        player: Bound<'py, PyAny>,
        channel: Bound<'py, PyAny>,
        database: Bound<'py, PyAny>,
        module: Bound<'py, PyModule>,
    }

    impl Fixtures<'_> {
        fn told(&self) -> Vec<String> {
            self.player
                .getattr("told")
                .and_then(|told| told.extract())
                .expect("could not get told messages")
        }

        fn stored(&self, key: &str) -> Option<String> {
            self.database
                .getattr("data")
                .and_then(|data| data.get_item(key))
                .and_then(|value| value.extract())
                .ok()
        }

        fn store(&self, key: &str, value: &str) {
            self.database
                .getattr("data")
                .and_then(|data| data.set_item(key, value))
                .expect("could not store the value");
        }

        fn register_server(&self, path: &str) {
            self.database
                .call_method1(intern!(self.database.py(), "sadd"), ("minqlx:motd", path))
                .expect("could not register the server");
        }

        fn stop_event() -> Option<Option<i32>> {
            Some(Some(STOP_EVENT))
        }
    }

    fn engine(cvars: &[(&'static str, &'static CStr)]) -> MockEngineBuilder {
        cvars
            .iter()
            .fold(MockEngineBuilder::default(), |engine, (name, value)| {
                let name = *name;
                let mut raw_cvar = CVarBuilder::default()
                    .string(value.as_ptr().cast_mut())
                    .build()
                    .expect("this should not happen");
                engine.with_find_cvar(
                    move |cmd| cmd == name,
                    move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                    0..,
                )
            })
    }

    fn with_motd<F>(sound: &'static CStr, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        let mut engine = engine(&[
            ("qlx_motdHeader", c"HEAD1\\nHEAD2"),
            ("qlx_motdSound", sound),
        ]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Pipeline:
    def __init__(self, database):
        self.database = database
        self.queued = []

    def set(self, key, value):
        self.queued.append((key, value))

    def execute(self):
        for key, value in self.queued:
            self.database.data[key] = value
        self.queued = []


class Database:
    def __init__(self, plugin):
        self.data = {}
        self.sets = {}
        self.flags = {}

    def sadd(self, key, *members):
        self.sets.setdefault(key, set()).update(members)

    def smembers(self, key):
        return set(self.sets.get(key, set()))

    def __setitem__(self, key, value):
        self.data[key] = value

    def __getitem__(self, key):
        return self.data[key]

    def __contains__(self, key):
        return key in self.data

    def __delitem__(self, key):
        del self.data[key]

    def delete(self, *keys):
        for key in keys:
            self.data.pop(key, None)

    def pipeline(self):
        return Pipeline(self)

    def get_flag(self, player, flag, default=False):
        return self.flags.get(flag, default)


class Player:
    steam_id = 1234

    def __init__(self):
        self.told = []

    def tell(self, message):
        self.told.append(message)


class Channel:
    pass


played = []


def record_sound(cls, sound, player):
    played.append((sound, player))


play_sound = classmethod(record_sound)
",
                    c"",
                    c"",
                )
                .expect("could not create the fixtures");
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                let _restore_sound =
                    RestoredAttribute::capture(plugin_class.as_any(), "play_sound");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                plugin_class
                    .setattr(
                        "play_sound",
                        module.getattr("play_sound").expect("no play_sound"),
                    )
                    .expect("could not record the sounds");

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Motd {
                        home: HOME.to_string(),
                        motd_key: KEY.to_string(),
                    }),
                )
                .expect("could not create the plugin");
                let database = plugin
                    .as_super()
                    .get_db()
                    .expect("could not get the database");
                let create = |name: &str| {
                    module
                        .getattr(name)
                        .and_then(|class| class.call0())
                        .expect("could not create the fixture")
                };
                test(Fixtures {
                    plugin,
                    player: create("Player"),
                    channel: create("Channel"),
                    database,
                    module,
                });
            });
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_motd_is_shown_below_the_header(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let result = Motd::send_motd(&fixtures.plugin, &fixtures.player, "LINE1\\nLINE2");

            assert!(result.is_ok());
            assert_eq!(fixtures.told(), ["HEAD1", "HEAD2", "LINE1", "LINE2"]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getmotd_without_a_motd_says_so(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let result = Motd::cmd_getmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["motd"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.told(), ["No MOTD has been set."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getmotd_shows_the_stored_motd(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            fixtures.store(KEY, "Welcome");

            let result = Motd::cmd_getmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["motd"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.told(), ["HEAD1", "HEAD2", "Welcome"]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmotd_needs_a_text(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let result = Motd::cmd_setmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["setmotd"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert_eq!(fixtures.stored(KEY), None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmotd_stores_the_text_and_registers_the_server(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let result = Motd::cmd_setmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["setmotd", "Have", "fun"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.stored(KEY), Some("Have fun".to_string()));
            assert_eq!(fixtures.told(), ["The MOTD has been set."]);
            assert!(
                fixtures
                    .database
                    .getattr("sets")
                    .and_then(|sets| sets.get_item("minqlx:motd"))
                    .and_then(|members| members.contains(HOME))
                    .unwrap_or(false)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmotdall_sets_the_text_for_every_registered_server(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            fixtures.register_server("/a");
            fixtures.register_server("/b");

            let result = Motd::cmd_setmotdall(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["setmotdall", "Hello", "all"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(
                fixtures.stored("minqlx:motd:/a"),
                Some("Hello all".to_string())
            );
            assert_eq!(
                fixtures.stored("minqlx:motd:/b"),
                Some("Hello all".to_string())
            );
            assert_eq!(fixtures.told(), ["All MOTDs have been set."]);
        });
    }

    #[rstest]
    #[case(Some("Welcome"), "The MOTD has been cleared.", None)]
    #[case(None, "No MOTD has been set.", None)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clearmotd_removes_the_motd_when_there_is_one(
        _pyshinqlx_setup: (),
        #[case] stored: Option<&str>,
        #[case] expected: &str,
        #[case] remaining: Option<&str>,
    ) {
        with_motd(c"0", |fixtures| {
            if let Some(motd) = stored {
                fixtures.store(KEY, motd);
            }

            let result = Motd::cmd_clearmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["clearmotd"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.told(), [expected]);
            assert_eq!(fixtures.stored(KEY).as_deref(), remaining);
        });
    }

    #[rstest]
    #[case(&["/a", "/b"])]
    #[case(&[])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clearmotdall_removes_the_motd_of_every_registered_server(
        _pyshinqlx_setup: (),
        #[case] servers: &[&str],
    ) {
        with_motd(c"0", |fixtures| {
            for server in servers {
                fixtures.register_server(server);
                fixtures.store(&format!("minqlx:motd:{server}"), "Welcome");
            }

            let result = Motd::cmd_clearmotdall(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["clearmotdall"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.told(), ["All MOTDs have been cleared."]);
            for server in servers {
                assert_eq!(fixtures.stored(&format!("minqlx:motd:{server}")), None);
            }
        });
    }

    #[rstest]
    #[case(None, "new", "new", "No MOTD was set, so a new one was made.")]
    #[case(Some(""), "new", "new", "No MOTD was set, so a new one was made.")]
    #[case(Some("old"), "new", "old new", "The MOTD has been updated.")]
    #[case(Some("old\\n"), "new", "old\\nnew", "The MOTD has been updated.")]
    #[case(Some("\\n"), "new", "\\n new", "The MOTD has been updated.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmotd_creates_or_extends_the_motd(
        _pyshinqlx_setup: (),
        #[case] stored: Option<&str>,
        #[case] addition: &str,
        #[case] expected_motd: &str,
        #[case] expected_message: &str,
    ) {
        with_motd(c"0", |fixtures| {
            if let Some(motd) = stored {
                fixtures.store(KEY, motd);
            }

            let result = Motd::cmd_addmotd(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["addmotd", addition]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(fixtures.stored(KEY), Some(expected_motd.to_string()));
            assert_eq!(fixtures.told(), [expected_message]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmotdall_extends_existing_and_creates_missing_motds(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            fixtures.register_server("/a");
            fixtures.register_server("/b");
            fixtures.store("minqlx:motd:/a", "old");

            let result = Motd::cmd_addmotdall(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["addmotdall", "new"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Fixtures::stop_event());
            assert_eq!(
                fixtures.stored("minqlx:motd:/a"),
                Some("old new".to_string())
            );
            assert_eq!(fixtures.stored("minqlx:motd:/b"), Some("new".to_string()));
            assert_eq!(fixtures.told(), ["Added to all MOTDs."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_loaded_player_gets_nothing_without_a_motd(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let result = Motd::deliver_motd(&fixtures.plugin, &fixtures.player);

            assert!(result.is_ok());
            assert!(fixtures.told().is_empty());
        });
    }

    fn played_sounds(fixtures: &Fixtures<'_>) -> usize {
        fixtures
            .module
            .getattr("played")
            .and_then(|played| played.len())
            .expect("could not count the sounds")
    }

    #[rstest]
    #[case(c"0", None, 0)]
    #[case(c"sound/welcome.wav", None, 1)]
    #[case(c"sound/welcome.wav", Some(false), 0)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_loaded_player_hears_the_welcome_sound_if_enabled(
        _pyshinqlx_setup: (),
        #[case] sound: &'static CStr,
        #[case] sounds_enabled: Option<bool>,
        #[case] expected_sounds: usize,
    ) {
        with_motd(sound, |fixtures| {
            fixtures.store(KEY, "Welcome");
            if let Some(enabled) = sounds_enabled {
                fixtures
                    .database
                    .getattr("flags")
                    .and_then(|flags| flags.set_item("essentials:sounds_enabled", enabled))
                    .expect("could not set the flag");
            }

            let result = Motd::deliver_motd(&fixtures.plugin, &fixtures.player);

            assert!(result.is_ok());
            assert_eq!(played_sounds(&fixtures), expected_sounds);
            assert_eq!(fixtures.told(), ["HEAD1", "HEAD2", "Welcome"]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_loaded_player_gets_the_motd_two_seconds_later(_pyshinqlx_setup: ()) {
        with_motd(c"0", |fixtures| {
            let py = fixtures.plugin.py();
            fixtures.store(KEY, "Welcome");
            let scheduler = py
                .import(intern!(py, "shinqlx"))
                .and_then(|shinqlx| shinqlx.getattr(intern!(py, "frame_tasks")))
                .expect("no frame tasks");
            let before = scheduler
                .getattr("queue")
                .and_then(|queue| queue.len())
                .expect("could not count the tasks");

            let result = Motd::handle_player_loaded(&fixtures.plugin, &fixtures.player);

            assert!(result.is_ok());
            assert!(fixtures.told().is_empty());
            let queue = scheduler.getattr("queue").expect("no queue");
            assert_eq!(queue.len().expect("no length"), before + 1);
            let task = queue.get_item(before).expect("could not get the new task");
            let action = task.getattr("action").expect("no action");
            let delay = task.getattr("time").expect("no time");
            assert!(delay.extract::<f64>().is_ok());
            let arguments = task.getattr("argument").expect("no arguments");
            let kwargs = task.getattr("kwargs").expect("no keyword arguments");
            action
                .call(
                    arguments
                        .cast::<pyo3::types::PyTuple>()
                        .expect("not a tuple"),
                    kwargs.cast::<pyo3::types::PyDict>().ok(),
                )
                .expect("could not run the delayed task");
            scheduler
                .call_method1("cancel", (task,))
                .expect("could not cancel the task");

            assert_eq!(fixtures.told(), ["HEAD1", "HEAD2", "Welcome"]);
        });
    }
}

#[cfg(test)]
mod motd_creation_tests {
    use core::{borrow::BorrowMut, ffi::CStr};

    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                COMMANDS, EVENT_DISPATCHERS, Plugin,
                commands::CommandInvoker,
                events::{
                    EventDispatcherManager, EventDispatcherManagerMethods, PlayerLoadedDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::RestoredAttribute,
            },
        },
        prelude::*,
    };

    fn engine(cvars: &[(&'static str, &'static CStr)]) -> MockEngineBuilder {
        cvars
            .iter()
            .fold(MockEngineBuilder::default(), |engine, (name, value)| {
                let name = *name;
                let mut raw_cvar = CVarBuilder::default()
                    .string(value.as_ptr().cast_mut())
                    .build()
                    .expect("this should not happen");
                engine.with_find_cvar(
                    move |cmd| cmd == name,
                    move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                    0..,
                )
            })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_registers_the_hook_the_commands_and_the_server(_pyshinqlx_setup: ()) {
        let mut engine = engine(&[
            ("fs_homepath", c"/ql"),
            ("zmq_stats_enable", c"0"),
            ("qlx_motdSound", c"0"),
            ("qlx_motdHeader", c"HEAD"),
        ]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        self.sets = {}

    def sadd(self, key, *members):
        self.sets.setdefault(key, set()).update(members)
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
                dispatchers
                    .add_dispatcher(&py.get_type::<PlayerLoadedDispatcher>())
                    .expect("could not add the player loaded dispatcher");
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
                assert_eq!(count("hooks"), 1);
                assert_eq!(count("commands"), 7);
                assert!(
                    plugin
                        .getattr("home")
                        .is_ok_and(|home| home.to_string() == "/ql")
                );
                assert!(
                    plugin
                        .getattr("motd_key")
                        .is_ok_and(|key| key.to_string() == "minqlx:motd:/ql")
                );
                assert!(
                    plugin
                        .getattr(intern!(py, "db"))
                        .and_then(|db| db.getattr("sets"))
                        .and_then(|sets| sets.get_item("minqlx:motd"))
                        .and_then(|members| members.contains("/ql"))
                        .unwrap_or(false)
                );
            });
        });
    }
}
