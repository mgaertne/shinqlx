use pyo3::{intern, prelude::*};

use super::{
    register_command,
    target::{reply, usage},
};
use crate::ffi::python::{AbstractChannel, Player, Plugin, next_frame, plugin::PluginMethods};

const MANAGER_PERMISSION: i32 = 5;

/// Loads, unloads and reloads plugins while the server is running.
#[pyclass(name = "plugin_manager", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct PluginManager {}

fn describe(py: Python<'_>, err: &PyErr) -> String {
    let name = err
        .get_type(py)
        .name()
        .map_or_else(|_| "Exception".to_owned(), |name| name.to_string());
    format!("{name} - {}", err.value(py))
}

fn shinqlx_function<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(intern!(py, "shinqlx"))?.getattr(name)
}

/// Runs the action for the plugin and tells the channel about the outcome.
fn report(
    slf: &Bound<'_, PluginManager>,
    channel: &Bound<'_, AbstractChannel>,
    plugin: &str,
    (success, failure): (&str, &str),
    action: &str,
) -> PyResult<Option<i32>> {
    let py = slf.py();
    match shinqlx_function(py, action).and_then(|action| action.call1((plugin,))) {
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
        report(slf, channel, plugin, ("loaded", "load"), "load_plugin")
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
            "unload_plugin",
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
        next_frame(&slf.getattr(intern!(py, "reload_in_next_frame"))?)?.call1((plugin, channel))?;
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
            "reload_plugin",
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
        next_frame(&slf.getattr(intern!(py, "load_all_in_next_frame"))?)?.call1((channel,))?;
        Ok(None)
    }

    fn load_all_in_next_frame(
        slf: &Bound<'_, Self>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        let py = slf.py();
        match shinqlx_function(py, "load_preset_plugins").and_then(|load| load.call0()) {
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
            if let Err(err) =
                shinqlx_function(py, "unload_plugin").and_then(|unload| unload.call1((&plugin,)))
            {
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
        next_frame(&slf.getattr(intern!(py, "reload_all_in_next_frame"))?)?.call1((channel,))?;
        Ok(None)
    }

    fn reload_all_in_next_frame(
        slf: &Bound<'_, Self>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        let py = slf.py();
        for plugin in other_plugins(slf)? {
            if let Err(err) =
                shinqlx_function(py, "reload_plugin").and_then(|reload| reload.call1((&plugin,)))
            {
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
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(PluginManager {}),
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
    use pyo3::{prelude::*, types::PyDict};
    use rstest::rstest;

    use super::{PluginManager, create};
    use crate::{
        ffi::python::{
            AbstractChannel, COMMANDS, EVENT_DISPATCHERS, Plugin, PythonReturnCodes,
            commands::CommandInvoker,
            events::EventDispatcherManager,
            native_plugins::test_support::{fixture_module, real_player},
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, run_all_frame_tasks},
        },
        logging::capture,
        prelude::*,
    };

    const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;

    const FIXTURES: &core::ffi::CStr = c"
calls = []
logged = []
failing = []


def make(name):
    def function(*args):
        calls.append((name, *args))
        if name in failing:
            raise RuntimeError('boom')
    return function


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
            let calls = self
                .module
                .getattr("calls")
                .expect("could not get the calls");
            calls
                .try_iter()
                .expect("not iterable")
                .map(|call| {
                    let call = call.expect("no call");
                    let name = call.get_item(0).and_then(|name| name.extract());
                    let plugin = call
                        .get_item(1)
                        .and_then(|plugin| plugin.extract())
                        .unwrap_or_default();
                    (name.expect("no name"), plugin)
                })
                .collect()
        }

        fn replies(&self) -> Vec<String> {
            self.channel
                .getattr("replies")
                .and_then(|replies| replies.extract())
                .expect("could not get the replies")
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
            self.module
                .getattr("failing")
                .and_then(|failing| failing.call_method1("append", (name,)))
                .expect("could not make the function fail");
        }
    }

    fn with_manager<F>(loaded: &[&str], test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        Python::attach(|py| {
            let module = fixture_module(py, FIXTURES);
            let shinqlx = py.import("shinqlx").expect("no shinqlx module");
            let mut restored = vec![];
            for name in [
                "load_plugin",
                "unload_plugin",
                "reload_plugin",
                "load_preset_plugins",
            ] {
                restored.push(RestoredAttribute::capture(shinqlx.as_any(), name));
                module
                    .setattr(
                        format!("real_{name}"),
                        shinqlx.getattr(name).expect("no such function"),
                    )
                    .expect("could not keep the real function");
                let fake = module
                    .getattr("make")
                    .and_then(|make| make.call1((name,)))
                    .expect("could not create the fake");
                shinqlx.setattr(name, fake).expect("could not replace it");
            }
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
                PyClassInitializer::from(Plugin::py_new(py)).add_subclass(PluginManager {}),
            )
            .expect("could not create the plugin");
            capture::install();
            test(Fixtures {
                mark: capture::mark(),
                plugin,
                channel: module
                    .getattr("Channel")
                    .and_then(|c| c.call0())
                    .and_then(|channel| channel.cast_into::<AbstractChannel>().map_err(Into::into))
                    .expect("no channel"),
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
        with_manager(&["ban"], |fixtures| {
            let py = fixtures.plugin.py();
            let shinqlx = py.import("shinqlx").expect("no shinqlx module");
            shinqlx
                .setattr(
                    "load_plugin",
                    fixtures
                        .module
                        .getattr("real_load_plugin")
                        .expect("no real function"),
                )
                .expect("could not use the real function");
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
            for (name, error, is_logged) in [
                ("real_load_plugin", "PluginLoadError", false),
                ("real_reload_plugin", "PluginLoadError", true),
            ] {
                let function = fixtures.module.getattr(name).expect("no real function");
                let before = fixtures.logged().len();

                assert!(function.call1(("no_such_plugin",)).is_err());

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
