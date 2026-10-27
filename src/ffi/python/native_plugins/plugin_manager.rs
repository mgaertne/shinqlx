use pyo3::{intern, prelude::*};

use super::{
    register_command,
    target::{reply, usage},
};
use crate::ffi::python::{Plugin, next_frame, plugin::PluginMethods};

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

fn log_exception(slf: &Bound<'_, PluginManager>) -> PyResult<()> {
    shinqlx_function(slf.py(), "log_exception")?
        .call1((slf,))
        .map(|_| ())
}

/// Runs the action for the plugin and tells the channel about the outcome.
fn report(
    slf: &Bound<'_, PluginManager>,
    channel: &Bound<'_, PyAny>,
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
            log_exception(slf)?;
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
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(plugin) = msg.get(1) else {
            return usage();
        };
        report(slf, channel, plugin, ("loaded", "load"), "load_plugin")
    }

    fn cmd_unload(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
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
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
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
        channel: &Bound<'_, PyAny>,
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
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        next_frame(&slf.getattr(intern!(py, "load_all_in_next_frame"))?)?.call1((channel,))?;
        Ok(None)
    }

    fn load_all_in_next_frame(slf: &Bound<'_, Self>, channel: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        match shinqlx_function(py, "load_preset_plugins").and_then(|load| load.call0()) {
            Ok(_) => reply(
                channel,
                "Successfully loaded all plugins in ^6qlx_plugins^7.",
            ),
            Err(err) => {
                let message = format!("Plugins failed to load: {}", describe(py, &err));
                log_exception(slf)?;
                reply(channel, &message)
            }
        }
        .map(|_| ())
    }

    fn cmd_unloadall(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
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
                log_exception(slf)?;
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
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        next_frame(&slf.getattr(intern!(py, "reload_all_in_next_frame"))?)?.call1((channel,))?;
        Ok(None)
    }

    fn reload_all_in_next_frame(slf: &Bound<'_, Self>, channel: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        for plugin in other_plugins(slf)? {
            if let Err(err) =
                shinqlx_function(py, "reload_plugin").and_then(|reload| reload.call1((&plugin,)))
            {
                let message = format!(
                    "Plugin ^6{plugin} ^7has failed to reload: {}",
                    describe(py, &err)
                );
                log_exception(slf)?;
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
            COMMANDS, EVENT_DISPATCHERS, Plugin, PythonReturnCodes,
            commands::CommandInvoker,
            events::EventDispatcherManager,
            native_plugins::test_support::fixture_module,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, run_all_frame_tasks},
        },
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


def log_exception(plugin=None):
    logged.append(plugin)
";

    struct Fixtures<'py> {
        plugin: Bound<'py, PluginManager>,
        module: Bound<'py, PyModule>,
        channel: Bound<'py, PyAny>,
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

        fn logged(&self) -> usize {
            self.module
                .getattr("logged")
                .and_then(|logged| logged.len())
                .expect("could not count the logged exceptions")
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
                let fake = module
                    .getattr("make")
                    .and_then(|make| make.call1((name,)))
                    .expect("could not create the fake");
                shinqlx.setattr(name, fake).expect("could not replace it");
            }
            restored.push(RestoredAttribute::capture(
                shinqlx.as_any(),
                "log_exception",
            ));
            shinqlx
                .setattr(
                    "log_exception",
                    module.getattr("log_exception").expect("no log_exception"),
                )
                .expect("could not replace log_exception");

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
            test(Fixtures {
                plugin,
                channel: module
                    .getattr("Channel")
                    .and_then(|c| c.call0())
                    .expect("no channel"),
                module,
            });
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
            let caller = py.None();
            let caller = caller.bind(py);

            for result in [
                PluginManager::cmd_load(
                    &fixtures.plugin,
                    caller,
                    msg(&["load"]),
                    &fixtures.channel,
                ),
                PluginManager::cmd_unload(
                    &fixtures.plugin,
                    caller,
                    msg(&["unload"]),
                    &fixtures.channel,
                ),
                PluginManager::cmd_reload(
                    &fixtures.plugin,
                    caller,
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
            let caller = py.None();
            let words = msg(&[command, "essentials"]);
            let result = if command == "load" {
                PluginManager::cmd_load(&fixtures.plugin, caller.bind(py), words, &fixtures.channel)
            } else {
                PluginManager::cmd_unload(
                    &fixtures.plugin,
                    caller.bind(py),
                    words,
                    &fixtures.channel,
                )
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
            let caller = py.None();
            fixtures.fail(&format!("{command}_plugin"));
            let words = msg(&[command, "essentials"]);
            let result = if command == "load" {
                PluginManager::cmd_load(&fixtures.plugin, caller.bind(py), words, &fixtures.channel)
            } else {
                PluginManager::cmd_unload(
                    &fixtures.plugin,
                    caller.bind(py),
                    words,
                    &fixtures.channel,
                )
            };

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                [format!(
                    "Plugin ^6essentials ^7has failed to {command}: RuntimeError - boom"
                )]
            );
            assert_eq!(fixtures.logged(), 1);
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
            let caller = py.None();
            if failing {
                fixtures.fail("reload_plugin");
            }

            let result = PluginManager::cmd_reload(
                &fixtures.plugin,
                caller.bind(py),
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
            let caller = py.None();
            if failing {
                fixtures.fail("load_preset_plugins");
            }

            let result = PluginManager::cmd_loadall(
                &fixtures.plugin,
                caller.bind(py),
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
            let caller = py.None();

            let result = PluginManager::cmd_unloadall(
                &fixtures.plugin,
                caller.bind(py),
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
            let caller = py.None();
            fixtures.fail("unload_plugin");

            let result = PluginManager::cmd_unloadall(
                &fixtures.plugin,
                caller.bind(py),
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
            assert_eq!(fixtures.logged(), 1);
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
            let caller = py.None();
            if failing {
                fixtures.fail("reload_plugin");
            }

            let result = PluginManager::cmd_reloadall(
                &fixtures.plugin,
                caller.bind(py),
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
