use pyo3::{intern, prelude::*};

use super::{
    register_command,
    target::{reply, usage},
};
use crate::ffi::python::{
    AbstractChannel, Player, Plugin, defer_next_frame,
    plugin::PluginMethods,
    plugin_loader::{load_plugin, load_preset_plugins, reload_plugin, unload_plugin},
};

const MANAGER_PERMISSION: i32 = 5;

/// What the manager does to plugins, so that the commands do not depend on the plugin loader.
#[derive(Clone, Copy)]
struct PluginActions {
    load: fn(Python<'_>, &str) -> PyResult<()>,
    unload: fn(Python<'_>, &str) -> PyResult<()>,
    reload: fn(Python<'_>, &str) -> PyResult<()>,
    load_preset: fn(Python<'_>) -> PyResult<()>,
}

impl PluginActions {
    const ENGINE: Self = Self {
        load: load_plugin,
        unload: unload_plugin,
        reload: reload_plugin,
        load_preset: load_preset_plugins,
    };
}

/// Loads, unloads and reloads plugins while the server is running.
#[pyclass(name = "plugin_manager", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct PluginManager {
    actions: PluginActions,
}

fn describe(py: Python<'_>, err: &PyErr) -> String {
    let name = err
        .get_type(py)
        .name()
        .map_or_else(|_| "Exception".to_owned(), |name| name.to_string());
    format!("{name} - {}", err.value(py))
}

/// Runs the action for the plugin and tells the channel about the outcome.
fn report(
    slf: &Bound<'_, PluginManager>,
    channel: &Bound<'_, AbstractChannel>,
    plugin: &str,
    (success, failure): (&str, &str),
    action: fn(Python<'_>, &str) -> PyResult<()>,
) -> PyResult<Option<i32>> {
    let py = slf.py();
    match action(py, plugin) {
        Ok(_) => reply(
            channel,
            &format!("Plugin ^6{plugin} ^7has been successfully {success}."),
        ),
        Err(err) => {
            let message = format!(
                "Plugin ^6{plugin} ^7has failed to {failure}: {}",
                describe(py, &err)
            );
            reply(channel, &message)
        }
    }
}

fn other_plugins(slf: &Bound<'_, PluginManager>) -> PyResult<Vec<String>> {
    let own_name = slf.as_super().get_name()?;
    Ok(slf
        .as_super()
        .get_plugins()?
        .keys()
        .extract::<Vec<String>>()?
        .into_iter()
        .filter(|plugin| *plugin != own_name)
        .collect())
}

#[pymethods]
impl PluginManager {
    fn cmd_load(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(plugin) = msg.get(1) else {
            return usage();
        };
        report(
            slf,
            channel,
            plugin,
            ("loaded", "load"),
            slf.get().actions.load,
        )
    }

    fn cmd_unload(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(plugin) = msg.get(1) else {
            return usage();
        };
        report(
            slf,
            channel,
            plugin,
            ("unloaded", "unload"),
            slf.get().actions.unload,
        )
    }

    fn cmd_reload(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(plugin) = msg.get(1) else {
            return usage();
        };
        let py = slf.py();
        defer_next_frame(
            &slf.getattr(intern!(py, "reload_in_next_frame"))?,
            (plugin, channel),
        )?;
        Ok(None)
    }

    fn reload_in_next_frame(
        slf: &Bound<'_, Self>,
        plugin: &str,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        report(
            slf,
            channel,
            plugin,
            ("reloaded", "reload"),
            slf.get().actions.reload,
        )
        .map(|_| ())
    }

    fn cmd_loadall(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        defer_next_frame(
            &slf.getattr(intern!(py, "load_all_in_next_frame"))?,
            (channel,),
        )?;
        Ok(None)
    }

    fn load_all_in_next_frame(
        slf: &Bound<'_, Self>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        let py = slf.py();
        match (slf.get().actions.load_preset)(py) {
            Ok(_) => reply(
                channel,
                "Successfully loaded all plugins in ^6qlx_plugins^7.",
            ),
            Err(err) => {
                let message = format!("Plugins failed to load: {}", describe(py, &err));
                reply(channel, &message)
            }
        }
        .map(|_| ())
    }

    fn cmd_unloadall(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        for plugin in other_plugins(slf)? {
            if let Err(err) = (slf.get().actions.unload)(py, &plugin) {
                let message = format!(
                    "Plugin ^6{plugin} ^7has failed to unload: {}",
                    describe(py, &err)
                );
                reply(channel, &message)?;
            }
        }

        reply(
            channel,
            &format!(
                "Successfully unloaded all plugins except {}.",
                slf.as_super().get_name()?
            ),
        )
    }

    fn cmd_reloadall(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        defer_next_frame(
            &slf.getattr(intern!(py, "reload_all_in_next_frame"))?,
            (channel,),
        )?;
        Ok(None)
    }

    fn reload_all_in_next_frame(
        slf: &Bound<'_, Self>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        let py = slf.py();
        for plugin in other_plugins(slf)? {
            if let Err(err) = (slf.get().actions.reload)(py, &plugin) {
                let message = format!(
                    "Plugin ^6{plugin} ^7has failed to reload: {}",
                    describe(py, &err)
                );
                reply(channel, &message)?;
            }
        }

        reply(
            channel,
            &format!(
                "Successfully reloaded all plugins except {}.",
                slf.as_super().get_name()?
            ),
        )
        .map(|_| ())
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(PluginManager {
            actions: PluginActions::ENGINE,
        }),
    )?;

    for (name, handler, usage) in [
        ("load", "cmd_load", "<plugin>"),
        ("unload", "cmd_unload", "<plugin>"),
        ("reload", "cmd_reload", "<plugin>"),
        ("loadall", "cmd_loadall", ""),
        ("unloadall", "cmd_unloadall", ""),
        ("reloadall", "cmd_reloadall", ""),
    ] {
        register_command(
            plugin.as_super(),
            &[name],
            handler,
            MANAGER_PERMISSION,
            usage,
            None,
        )?;
    }

    Ok(plugin.into_any())
}

#[cfg(test)]
mod plugin_manager_tests {
    use std::sync::Mutex;

    use pyo3::{exceptions::PyRuntimeError, prelude::*, types::PyDict};
    use rstest::rstest;

    use super::{PluginActions, PluginManager, create};
    use crate::{
        ffi::python::{
            AbstractChannel, COMMANDS, EVENT_DISPATCHERS, Plugin, PythonReturnCodes,
            commands::CommandInvoker,
            events::EventDispatcherManager,
            native_plugins::test_support::{fixture_module, real_player},
            plugin_loader::{load_plugin, reload_plugin},
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{
                RestoredAttribute, recorded_replies, recording_channel, run_all_frame_tasks,
            },
        },
        logging::capture,
        prelude::*,
    };

    const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;

    static CALLS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    static FAILING: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn record(name: &str, plugin: &str) -> PyResult<()> {
        CALLS
            .lock()
            .expect("the calls are not poisoned")
            .push((name.to_owned(), plugin.to_owned()));
        if FAILING
            .lock()
            .expect("the failing names are not poisoned")
            .iter()
            .any(|failing| failing == name)
        {
            return Err(PyRuntimeError::new_err("boom"));
        }
        Ok(())
    }

    fn fake_load(_py: Python<'_>, plugin: &str) -> PyResult<()> {
        record("load_plugin", plugin)
    }

    fn fake_unload(_py: Python<'_>, plugin: &str) -> PyResult<()> {
        record("unload_plugin", plugin)
    }

    fn fake_reload(_py: Python<'_>, plugin: &str) -> PyResult<()> {
        record("reload_plugin", plugin)
    }

    fn fake_load_preset(_py: Python<'_>) -> PyResult<()> {
        record("load_preset_plugins", "")
    }

    const FAKE_ACTIONS: PluginActions = PluginActions {
        load: fake_load,
        unload: fake_unload,
        reload: fake_reload,
        load_preset: fake_load_preset,
    };

    const FIXTURES: &core::ffi::CStr = c"
logged = []

import logging


class Collector(logging.Handler):
    def emit(self, record):
        logged.append(record.getMessage())


collector = Collector()


def install():
    logging.getLogger('shinqlx').addHandler(collector)


def uninstall():
    logging.getLogger('shinqlx').removeHandler(collector)
";

    struct Fixtures<'py> {
        plugin: Bound<'py, PluginManager>,
        module: Bound<'py, PyModule>,
        channel: Bound<'py, AbstractChannel>,
        mark: usize,
    }

    impl Fixtures<'_> {
        fn calls(&self) -> Vec<(String, String)> {
            CALLS.lock().expect("the calls are not poisoned").clone()
        }

        fn replies(&self) -> Vec<String> {
            recorded_replies(&self.channel)
        }

        fn logged(&self) -> Vec<String> {
            let python_logged = self
                .module
                .getattr("logged")
                .and_then(|logged| logged.extract::<Vec<String>>())
                .expect("could not get the logged exceptions");
            let rust_logged = capture::records_since(self.mark)
                .into_iter()
                .map(|(level, _, line)| format!("{level}: {line}"));
            python_logged.into_iter().chain(rust_logged).collect()
        }

        fn fail(&self, name: &str) {
            FAILING
                .lock()
                .expect("the failing names are not poisoned")
                .push(name.to_owned());
        }
    }

    fn with_manager<F>(loaded: &[&str], test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_actions(loaded, FAKE_ACTIONS, test);
    }

    fn with_actions<F>(loaded: &[&str], actions: PluginActions, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        Python::attach(|py| {
            CALLS.lock().expect("the calls are not poisoned").clear();
            FAILING
                .lock()
                .expect("the failing names are not poisoned")
                .clear();
            let module = fixture_module(py, FIXTURES);
            let mut restored = vec![];
            module
                .call_method0("install")
                .expect("could not install the log collector");

            let plugin_class = py.get_type::<Plugin>();
            restored.push(RestoredAttribute::capture(
                plugin_class.as_any(),
                "_loaded_plugins",
            ));
            let plugins = PyDict::new(py);
            for name in loaded {
                plugins.set_item(name, py.None()).expect("could not add");
            }
            plugin_class
                .setattr("_loaded_plugins", plugins)
                .expect("could not set the plugins");

            let plugin = Bound::new(
                py,
                PyClassInitializer::from(Plugin::py_new(py))
                    .add_subclass(PluginManager { actions }),
            )
            .expect("could not create the plugin");
            capture::install();
            test(Fixtures {
                mark: capture::mark(),
                plugin,
                channel: recording_channel(py),
                module: module.clone(),
            });
            module
                .call_method0("uninstall")
                .expect("could not remove the log collector");
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn commands_with_a_plugin_ask_for_usage_without_one(_pyshinqlx_setup: ()) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");

            for result in [
                PluginManager::cmd_load(
                    &fixtures.plugin,
                    &caller,
                    msg(&["load"]),
                    &fixtures.channel,
                ),
                PluginManager::cmd_unload(
                    &fixtures.plugin,
                    &caller,
                    msg(&["unload"]),
                    &fixtures.channel,
                ),
                PluginManager::cmd_reload(
                    &fixtures.plugin,
                    &caller,
                    msg(&["reload"]),
                    &fixtures.channel,
                ),
            ] {
                assert_eq!(result.ok(), Some(Some(USAGE)));
            }
            assert!(fixtures.calls().is_empty());
        });
    }

    #[rstest]
    #[case("load", "has been successfully loaded.")]
    #[case("unload", "has been successfully unloaded.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_and_unload_report_their_success(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] expected: &str,
    ) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            let words = msg(&[command, "essentials"]);
            let result = if command == "load" {
                PluginManager::cmd_load(&fixtures.plugin, &caller, words, &fixtures.channel)
            } else {
                PluginManager::cmd_unload(&fixtures.plugin, &caller, words, &fixtures.channel)
            };

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.calls(),
                [(format!("{command}_plugin"), "essentials".to_owned())]
            );
            assert_eq!(
                fixtures.replies(),
                [format!("Plugin ^6essentials ^7{expected}")]
            );
        });
    }

    #[rstest]
    #[case("load")]
    #[case("unload")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_and_unload_report_their_failure(_pyshinqlx_setup: (), #[case] command: &str) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            fixtures.fail(&format!("{command}_plugin"));
            let words = msg(&[command, "essentials"]);
            let result = if command == "load" {
                PluginManager::cmd_load(&fixtures.plugin, &caller, words, &fixtures.channel)
            } else {
                PluginManager::cmd_unload(&fixtures.plugin, &caller, words, &fixtures.channel)
            };

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                [format!(
                    "Plugin ^6essentials ^7has failed to {command}: RuntimeError - boom"
                )]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn refused_load_is_replied_and_not_logged(_pyshinqlx_setup: ()) {
        with_actions(&["ban"], PluginActions::ENGINE, |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");

            let result = PluginManager::cmd_load(
                &fixtures.plugin,
                &caller,
                msg(&["load", "r!ban"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                [
                    "Plugin ^6r!ban ^7has failed to load: PluginLoadError - Plugin 'ban' is already loaded as a Python plugin. Unload it first."
                ]
            );
            let logged = fixtures.logged();
            let count = |text: &str| logged.iter().filter(|line| line.contains(text)).count();
            assert_eq!(
                count("is already loaded as a Python plugin"),
                0,
                "{logged:?}"
            );
            assert!(
                logged.iter().all(|line| !line.contains("NoneType")),
                "{logged:?}"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_logs_its_failure_and_load_only_raises_for_a_missing_plugin(_pyshinqlx_setup: ()) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            for (name, function, error, is_logged) in [
                (
                    "load_plugin",
                    load_plugin as fn(Python<'_>, &str) -> PyResult<()>,
                    "PluginLoadError",
                    false,
                ),
                ("reload_plugin", reload_plugin, "PluginLoadError", true),
            ] {
                let before = fixtures.logged().len();

                assert!(function(py, "no_such_plugin").is_err());

                let logged = fixtures.logged();
                assert_eq!(
                    logged[before..].iter().any(|line| line.contains(error)),
                    is_logged,
                    "{name}: {logged:?}"
                );
                assert!(
                    logged.iter().all(|line| !line.contains("NoneType")),
                    "{name}: {logged:?}"
                );
            }
        });
    }

    #[rstest]
    #[case(false, "Plugin ^6silence ^7has been successfully reloaded.")]
    #[case(true, "Plugin ^6silence ^7has failed to reload: RuntimeError - boom")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_happens_in_the_next_frame(
        _pyshinqlx_setup: (),
        #[case] failing: bool,
        #[case] expected: &str,
    ) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            if failing {
                fixtures.fail("reload_plugin");
            }

            let result = PluginManager::cmd_reload(
                &fixtures.plugin,
                &caller,
                msg(&["reload", "silence"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert!(fixtures.calls().is_empty());
            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(
                fixtures.calls(),
                [("reload_plugin".to_owned(), "silence".to_owned())]
            );
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[case(false, "Successfully loaded all plugins in ^6qlx_plugins^7.")]
    #[case(true, "Plugins failed to load: RuntimeError - boom")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn loadall_happens_in_the_next_frame(
        _pyshinqlx_setup: (),
        #[case] failing: bool,
        #[case] expected: &str,
    ) {
        with_manager(&[], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            if failing {
                fixtures.fail("load_preset_plugins");
            }

            let result = PluginManager::cmd_loadall(
                &fixtures.plugin,
                &caller,
                msg(&["loadall"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert!(fixtures.replies().is_empty());
            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unloadall_unloads_every_other_plugin(_pyshinqlx_setup: ()) {
        with_manager(&["plugin_manager", "silence", "ban"], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");

            let result = PluginManager::cmd_unloadall(
                &fixtures.plugin,
                &caller,
                msg(&["unloadall"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.calls(),
                [
                    ("unload_plugin".to_owned(), "silence".to_owned()),
                    ("unload_plugin".to_owned(), "ban".to_owned())
                ]
            );
            assert_eq!(
                fixtures.replies(),
                ["Successfully unloaded all plugins except plugin_manager."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unloadall_reports_plugins_that_fail_to_unload(_pyshinqlx_setup: ()) {
        with_manager(&["silence"], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            fixtures.fail("unload_plugin");

            let result = PluginManager::cmd_unloadall(
                &fixtures.plugin,
                &caller,
                msg(&["unloadall"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                [
                    "Plugin ^6silence ^7has failed to unload: RuntimeError - boom",
                    "Successfully unloaded all plugins except plugin_manager."
                ]
            );
        });
    }

    #[rstest]
    #[case(false)]
    #[case(true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reloadall_reloads_every_other_plugin_in_the_next_frame(
        _pyshinqlx_setup: (),
        #[case] failing: bool,
    ) {
        with_manager(&["plugin_manager", "silence"], |fixtures| {
            let py = fixtures.plugin.py();
            let caller = real_player(py, 1234, "Ada^7", "");
            if failing {
                fixtures.fail("reload_plugin");
            }

            let result = PluginManager::cmd_reloadall(
                &fixtures.plugin,
                &caller,
                msg(&["reloadall"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert!(fixtures.calls().is_empty());
            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(
                fixtures.calls(),
                [("reload_plugin".to_owned(), "silence".to_owned())]
            );
            let replies = fixtures.replies();
            assert_eq!(
                replies.last().map(String::as_str),
                Some("Successfully reloaded all plugins except plugin_manager.")
            );
            assert_eq!(replies.len(), if failing { 2 } else { 1 });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_registers_the_commands(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatchers =
                Bound::new(py, EventDispatcherManager::default()).expect("no dispatchers");
            EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));
            COMMANDS.store(Some(
                Py::new(py, CommandInvoker::py_new())
                    .expect("could not create the command invoker")
                    .into(),
            ));

            let plugin = create(py).expect("could not create the plugin");

            assert_eq!(
                plugin
                    .getattr("commands")
                    .and_then(|commands| commands.len())
                    .expect("could not count"),
                6
            );
            assert_eq!(
                plugin
                    .getattr("hooks")
                    .and_then(|hooks| hooks.len())
                    .expect("could not count"),
                0
            );
        });
    }
}
