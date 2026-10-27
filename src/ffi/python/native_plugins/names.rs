use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::{intern, prelude::*, types::PyDict};

use super::{cvar_flag, register_client_command, register_hook, tell, without_excessive_colors};
use crate::ffi::python::{
    Plugin, PythonReturnCodes, clean_text, commands::CommandPriorities, get_cvar,
    plugin::PluginMethods,
};

const STOP_ALL: i32 = PythonReturnCodes::RET_STOP_ALL as i32;
const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;
const MAX_NAME_BYTES: usize = 36;
const ENFORCE_STEAM_NAME_CVAR: &str = "qlx_enforceSteamName";

/// Lets players register a colored version of their Steam name.
#[pyclass(name = "names", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Names {
    steam_names: Mutex<HashMap<i64, String>>,
    name_set: AtomicBool,
}

impl Names {
    fn new() -> Self {
        Self {
            steam_names: Mutex::new(HashMap::new()),
            name_set: AtomicBool::new(false),
        }
    }
}

fn name_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}:colored_name")
}

fn steam_id_of(player: &Bound<'_, PyAny>) -> PyResult<i64> {
    player.getattr(intern!(player.py(), "steam_id"))?.extract()
}

fn clean_name_of(player: &Bound<'_, PyAny>) -> PyResult<String> {
    player
        .getattr(intern!(player.py(), "clean_name"))?
        .extract()
}

fn matches_steam_name(name: &str, player: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(clean_text(&name).to_lowercase() == clean_name_of(player)?.to_lowercase())
}

#[pymethods]
impl Names {
    fn handle_player_connect(&self, player: &Bound<'_, PyAny>) -> PyResult<()> {
        self.steam_names
            .lock()
            .insert(steam_id_of(player)?, clean_name_of(player)?);
        Ok(())
    }

    fn handle_player_loaded(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let key = name_key(steam_id_of(player)?);
        if !database.contains(&key)? {
            return Ok(());
        }

        let registered_name = database.get_item(&key)?.extract::<String>()?;
        if !cvar_flag(py, ENFORCE_STEAM_NAME_CVAR)? || matches_steam_name(&registered_name, player)?
        {
            slf.get().name_set.store(true, Ordering::Release);
            player.setattr(intern!(py, "name"), registered_name)?;
        }
        Ok(())
    }

    fn handle_player_disconnect(
        &self,
        player: &Bound<'_, PyAny>,
        _reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.steam_names.lock().remove(&steam_id_of(player)?);
        Ok(())
    }

    fn handle_userinfo<'py>(
        slf: &Bound<'py, Self>,
        player: &Bound<'py, PyAny>,
        changed: &Bound<'py, PyDict>,
    ) -> PyResult<Option<Bound<'py, PyDict>>> {
        let names = slf.get();
        if names.name_set.swap(false, Ordering::AcqRel) {
            return Ok(None);
        }
        let Some(new_name) = changed.get_item(intern!(slf.py(), "name"))? else {
            return Ok(None);
        };

        let steam_id = steam_id_of(player)?;
        let database = slf.as_super().get_db()?;
        let key = name_key(steam_id);
        let new_clean_name = clean_text(&new_name.extract::<String>()?);
        if !database.contains(&key)? {
            names.steam_names.lock().insert(steam_id, new_clean_name);
            return Ok(None);
        }

        let current_clean_name = clean_name_of(player)?;
        let steam_name = names
            .steam_names
            .lock()
            .entry(steam_id)
            .or_insert(current_clean_name)
            .clone();
        if steam_name == new_clean_name {
            changed.set_item(intern!(slf.py(), "name"), database.get_item(&key)?)?;
            return Ok(Some(changed.clone()));
        }

        database.del_item(&key)?;
        tell(player, "Your registered name has been reset.")?;
        Ok(None)
    }

    fn cmd_name(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let database = slf.as_super().get_db()?;
        let key = name_key(steam_id_of(player)?);

        if msg.len() < 2 {
            if !database.contains(&key)? {
                return Ok(Some(USAGE));
            }
            database.del_item(&key)?;
            tell(player, "Your registered name has been removed.")?;
            return Ok(Some(STOP_ALL));
        }

        let name = without_excessive_colors(&msg[1..].join(" "));
        let rejection = if name.len() > MAX_NAME_BYTES {
            Some("The name is too long. Consider using fewer colors or a shorter name.")
        } else if !matches_steam_name(&name, player)? && cvar_flag(py, ENFORCE_STEAM_NAME_CVAR)? {
            Some("The new name must match your current Steam name.")
        } else if name.contains('\\') {
            Some("The character '^6\\^7' cannot be used. Sorry for the inconvenience.")
        } else if clean_text(&name).trim().is_empty() {
            Some("Blank names cannot be used. Sorry for the inconvenience.")
        } else {
            None
        };
        if let Some(rejection) = rejection {
            tell(player, rejection)?;
            return Ok(Some(STOP_ALL));
        }

        let name = format!("^7{name}");
        slf.get().name_set.store(true, Ordering::Release);
        player.setattr(intern!(py, "name"), &name)?;
        database.set_item(&key, &name)?;
        let prefix = get_cvar("qlx_commandPrefix")?.unwrap_or_default();
        tell(
            player,
            &format!(
                "The name has been registered. To make me forget about it, a simple ^6{prefix}name^7 will do it."
            ),
        )?;
        Ok(Some(STOP_ALL))
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Names::new()),
    )?;

    for (event, handler) in [
        ("player_connect", "handle_player_connect"),
        ("player_loaded", "handle_player_loaded"),
        ("player_disconnect", "handle_player_disconnect"),
        ("userinfo", "handle_userinfo"),
    ] {
        register_hook(
            plugin.as_super(),
            event,
            handler,
            CommandPriorities::PRI_NORMAL,
        )?;
    }
    register_client_command(
        plugin.as_super(),
        &["name", "setname"],
        "cmd_name",
        "<name>",
    )?;

    py.get_type::<Plugin>()
        .call_method1(intern!(py, "set_cvar_once"), (ENFORCE_STEAM_NAME_CVAR, "1"))?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod names_tests {
    use core::sync::atomic::Ordering;

    use pyo3::{
        prelude::*,
        types::{PyDict, PyDictMethods},
    };
    use rstest::rstest;

    use super::{Names, STOP_ALL, USAGE, name_key};
    use crate::{
        ffi::python::{
            Plugin,
            plugin::PluginMethods,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, engine_with_cvars},
        },
        prelude::*,
    };

    const STEAM_ID: i64 = 76_561_198_000_000_001;

    struct Fixtures<'py> {
        plugin: Bound<'py, Names>,
        player: Bound<'py, PyAny>,
        channel: Bound<'py, PyAny>,
        database: Bound<'py, PyAny>,
    }

    impl Fixtures<'_> {
        fn told(&self) -> Vec<String> {
            self.player
                .getattr("told")
                .and_then(|told| told.extract())
                .expect("could not get told messages")
        }

        fn player_name(&self) -> String {
            self.player
                .getattr("name")
                .and_then(|name| name.extract())
                .expect("could not get the name")
        }

        fn stored(&self) -> Option<String> {
            self.database
                .getattr("data")
                .and_then(|data| data.get_item(name_key(STEAM_ID)))
                .and_then(|value| value.extract())
                .ok()
        }

        fn store(&self, name: &str) {
            self.database
                .getattr("data")
                .and_then(|data| data.set_item(name_key(STEAM_ID), name))
                .expect("could not store the name");
        }

        fn steam_name(&self) -> Option<String> {
            self.plugin.get().steam_names.lock().get(&STEAM_ID).cloned()
        }

        fn remember_steam_name(&self, name: &str) {
            self.plugin
                .get()
                .steam_names
                .lock()
                .insert(STEAM_ID, name.to_owned());
        }

        fn name_set(&self) -> bool {
            self.plugin.get().name_set.load(Ordering::Acquire)
        }
    }

    fn with_names<F>(enforce: &'static std::ffi::CStr, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        let mut engine = engine_with_cvars(&[
            ("qlx_enforceSteamName", enforce),
            ("qlx_commandPrefix", c"!"),
        ]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        self.data = {}

    def __setitem__(self, key, value):
        self.data[key] = value

    def __getitem__(self, key):
        return self.data[key]

    def __contains__(self, key):
        return key in self.data

    def __delitem__(self, key):
        del self.data[key]


class Player:
    steam_id = 76561198000000001
    clean_name = 'Ada'

    def __init__(self):
        self.name = 'Ada^7'
        self.told = []

    def tell(self, message):
        self.told.append(message)


class Channel:
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

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Names::new()),
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
                });
            });
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn changed<'py>(py: Python<'py>, name: &str) -> Bound<'py, PyDict> {
        let changed = PyDict::new(py);
        changed
            .set_item("name", name)
            .expect("could not set the name");
        changed
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connecting_remembers_the_steam_name(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            let result = fixtures
                .plugin
                .get()
                .handle_player_connect(&fixtures.player);

            assert!(result.is_ok());
            assert_eq!(fixtures.steam_name().as_deref(), Some("Ada"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn disconnecting_forgets_the_steam_name(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures.remember_steam_name("Ada");

            let result = fixtures
                .plugin
                .get()
                .handle_player_disconnect(&fixtures.player, &fixtures.channel);

            assert!(result.is_ok());
            assert_eq!(fixtures.steam_name(), None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn loading_without_a_registered_name_changes_nothing(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            let result = Names::handle_player_loaded(&fixtures.plugin, &fixtures.player);

            assert!(result.is_ok());
            assert_eq!(fixtures.player_name(), "Ada^7");
            assert!(!fixtures.name_set());
        });
    }

    #[rstest]
    #[case(c"1", "^1A^2da", true)]
    #[case(c"1", "^1Eve", false)]
    #[case(c"0", "^1Eve", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn loading_applies_the_registered_name_if_allowed(
        _pyshinqlx_setup: (),
        #[case] enforce: &'static std::ffi::CStr,
        #[case] registered: &str,
        #[case] applied: bool,
    ) {
        with_names(enforce, |fixtures| {
            fixtures.store(registered);

            let result = Names::handle_player_loaded(&fixtures.plugin, &fixtures.player);

            assert!(result.is_ok());
            assert_eq!(fixtures.name_set(), applied);
            assert_eq!(
                fixtures.player_name(),
                if applied { registered } else { "Ada^7" }
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_changes_made_by_the_plugin_are_ignored_once(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures
                .plugin
                .get()
                .name_set
                .store(true, Ordering::Release);

            Python::attach(|py| {
                let result =
                    Names::handle_userinfo(&fixtures.plugin, &fixtures.player, &changed(py, "Eve"));

                assert!(result.is_ok_and(|result| result.is_none()));
            });
            assert!(!fixtures.name_set());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_without_a_name_changes_nothing(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            Python::attach(|py| {
                let result =
                    Names::handle_userinfo(&fixtures.plugin, &fixtures.player, &PyDict::new(py));

                assert!(result.is_ok_and(|result| result.is_none()));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_without_a_registered_name_remembers_the_steam_name(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            Python::attach(|py| {
                let result = Names::handle_userinfo(
                    &fixtures.plugin,
                    &fixtures.player,
                    &changed(py, "^1Eve^7"),
                );

                assert!(result.is_ok_and(|result| result.is_none()));
            });
            assert_eq!(fixtures.steam_name().as_deref(), Some("Eve"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_with_the_unchanged_steam_name_restores_the_registered_name(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures.store("^7^1A^2da");
            fixtures.remember_steam_name("Ada");

            Python::attach(|py| {
                let result =
                    Names::handle_userinfo(&fixtures.plugin, &fixtures.player, &changed(py, "Ada"));

                let returned = result
                    .expect("the handler failed")
                    .expect("the userinfo should have been changed");
                assert!(
                    returned
                        .get_item("name")
                        .is_ok_and(|name| name.is_some_and(|name| name.to_string() == "^7^1A^2da"))
                );
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_with_an_unknown_steam_name_falls_back_to_the_current_one(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures.store("^7^1A^2da");

            Python::attach(|py| {
                let result =
                    Names::handle_userinfo(&fixtures.plugin, &fixtures.player, &changed(py, "Ada"));

                assert!(result.is_ok_and(|result| result.is_some()));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn userinfo_with_a_new_steam_name_resets_the_registration(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures.store("^7^1A^2da");
            fixtures.remember_steam_name("Ada");

            Python::attach(|py| {
                let result =
                    Names::handle_userinfo(&fixtures.plugin, &fixtures.player, &changed(py, "Eve"));

                assert!(result.is_ok_and(|result| result.is_none()));
            });
            assert_eq!(fixtures.stored(), None);
            assert_eq!(fixtures.told(), ["Your registered name has been reset."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn name_without_arguments_and_without_registration_asks_for_usage(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            let result = Names::cmd_name(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["name"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert!(fixtures.told().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn name_without_arguments_removes_the_registration(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            fixtures.store("^7^1Ada");

            let result = Names::cmd_name(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["name"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.stored(), None);
            assert_eq!(fixtures.told(), ["Your registered name has been removed."]);
        });
    }

    #[rstest]
    #[case(c"1", &["name", "^1A^2d^3a"], "The name has been registered.", true)]
    #[case(c"0", &["name", "Eve"], "The name has been registered.", true)]
    #[case(
        c"1",
        &["name", "Eve"],
        "The new name must match your current Steam name.",
        false
    )]
    #[case(
        c"0",
        &["name", "^1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        "The name is too long. Consider using fewer colors or a shorter name.",
        false
    )]
    #[case(
        c"0",
        &["name", "Ev\\e"],
        "The character '^6\\^7' cannot be used. Sorry for the inconvenience.",
        false
    )]
    #[case(
        c"0",
        &["name", "^1", "^2"],
        "Blank names cannot be used. Sorry for the inconvenience.",
        false
    )]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn name_registers_valid_names_and_rejects_the_others(
        _pyshinqlx_setup: (),
        #[case] enforce: &'static std::ffi::CStr,
        #[case] words: &[&str],
        #[case] told_prefix: &str,
        #[case] registered: bool,
    ) {
        with_names(enforce, |fixtures| {
            let result = Names::cmd_name(
                &fixtures.plugin,
                &fixtures.player,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert!(
                fixtures
                    .told()
                    .first()
                    .is_some_and(|told| told.starts_with(told_prefix)),
                "{:?}",
                fixtures.told()
            );
            assert_eq!(fixtures.stored().is_some(), registered);
            assert_eq!(fixtures.name_set(), registered);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn name_stores_the_cleaned_name_with_a_color_reset_in_front(_pyshinqlx_setup: ()) {
        with_names(c"1", |fixtures| {
            let result = Names::cmd_name(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["name", "^1^2A^3d^4^5a"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.stored().as_deref(), Some("^7^2A^3d^5a"));
            assert_eq!(fixtures.player_name(), "^7^2A^3d^5a");
            assert_eq!(
                fixtures.told(),
                [
                    "The name has been registered. To make me forget about it, a simple ^6!name^7 will do it."
                ]
            );
        });
    }
}

#[cfg(test)]
mod names_creation_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::python::{
            COMMANDS, EVENT_DISPATCHERS, Plugin,
            commands::CommandInvoker,
            events::{
                EventDispatcherManager, EventDispatcherManagerMethods, PlayerConnectDispatcher,
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
    fn creating_the_plugin_registers_the_hooks_and_the_client_command(_pyshinqlx_setup: ()) {
        let mut engine =
            engine_with_cvars(&[("qlx_enforceSteamName", c"1"), ("zmq_stats_enable", c"0")]);
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
                    py.get_type::<PlayerConnectDispatcher>(),
                    py.get_type::<PlayerLoadedDispatcher>(),
                    py.get_type::<PlayerDisconnectDispatcher>(),
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

                assert!(
                    plugin
                        .getattr(intern!(py, "hooks"))
                        .and_then(|hooks| hooks.len())
                        .is_ok_and(|count| count == 4)
                );
                let commands = plugin
                    .getattr(intern!(py, "commands"))
                    .expect("could not get the commands");
                assert_eq!(commands.len().ok(), Some(1));
                assert!(
                    commands
                        .try_iter()
                        .expect("no iterator")
                        .flatten()
                        .all(|command| command
                            .getattr(intern!(py, "client_cmd_perm"))
                            .is_ok_and(|perm| perm.extract::<i32>().is_ok_and(|perm| perm == 0)))
                );
            });
        });
    }
}
