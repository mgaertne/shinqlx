use core::hint::cold_path;
use std::path::Path;

use itertools::Itertools;
use pyo3::{
    exceptions::{PyAttributeError, PyEnvironmentError},
    intern,
    types::{IntoPyDict, PyDict, PyString, PyTuple, PyType},
};
use tap::{TapFallible, TapOptional};

use super::{
    COMMANDS, EVENT_DISPATCHERS, MODULES, PluginLoadError, PluginUnloadError, decorators,
    event_dispatcher, log_exception, native_plugins, prelude::*, pyshinqlx_get_logger,
    try_get_plugins_path,
};
use crate::{MAIN_ENGINE, quake_live_engine::FindCVar};

pub(crate) static DEFAULT_PLUGINS: [&str; 10] = [
    "plugin_manager",
    "essentials",
    "motd",
    "permission",
    "ban",
    "silence",
    "clan",
    "names",
    "log",
    "workshop",
];

#[pyfunction(name = "load_preset_plugins")]
pub(super) fn load_preset_plugins(py: Python<'_>) -> PyResult<()> {
    try_get_plugins_path().map_err(PluginLoadError::new_err)?;

    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PluginLoadError::new_err("no main engine found"))
        },
        |main_engine| {
            main_engine
                .find_cvar("qlx_plugins")
                .tap_some(|plugins_cvar| {
                    let plugins_str = plugins_cvar.get_string();
                    let mut plugins = plugins_str
                        .split(',')
                        .map(|value| value.trim())
                        .collect::<Vec<_>>();
                    if plugins.contains(&"DEFAULT") {
                        plugins.extend_from_slice(&DEFAULT_PLUGINS);
                        plugins.retain(|&value| value != "DEFAULT");
                    }
                    plugins.iter().unique().for_each(|&plugin| {
                        let _ = load_plugin(py, plugin);
                    });
                });

            Ok(())
        },
    )
}

pub(super) fn try_load_plugin(py: Python<'_>, plugin: &str, plugins_path: &Path) -> PyResult<()> {
    let Some(plugins_dir) = plugins_path
        .file_name()
        .map(|plugins_dir| plugins_dir.to_string_lossy())
    else {
        cold_path();
        return Err(PluginLoadError::new_err(
            "qlx_pluginsPath is not pointing to an existing directory",
        ));
    };

    let plugin_import_path = format!("{}.{}", plugins_dir, plugin);
    let module = py
        .import(intern!(py, "importlib"))
        .and_then(|importlib_module| {
            importlib_module.call_method1(intern!(py, "import_module"), (plugin_import_path,))
        })?;

    let plugin_pystring = PyString::intern(py, plugin);
    if let Some(modules) = MODULES.load().as_ref() {
        modules.bind(py).set_item(&plugin_pystring, &module)?;
    }

    if !module.hasattr(&plugin_pystring)? {
        cold_path();
        return Err(PluginLoadError::new_err(
            "The plugin needs to have a class with the exact name as the file, minus the .py.",
        ));
    }

    let plugin_class = module.getattr(&plugin_pystring)?;
    let plugin_type = py.get_type::<Plugin>();
    if !plugin_class
        .cast::<PyType>()?
        .is_subclass(&plugin_type)
        .unwrap_or(false)
    {
        cold_path();
        return Err(PluginLoadError::new_err(
            "Attempted to load a plugin that is not a subclass of 'shinqlx.Plugin'.",
        ));
    }

    plugin_class.call0().and_then(|loaded_plugin| {
        decorators::register_decorated_handlers(&loaded_plugin)?;

        let loaded_plugins = py
            .get_type::<Plugin>()
            .getattr(intern!(py, "_loaded_plugins"))?;
        loaded_plugins
            .cast::<PyDict>()?
            .set_item(plugin, loaded_plugin)
    })
}

#[pyfunction(name = "load_plugin")]
pub(crate) fn load_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    load_plugin_with(py, plugin, true)
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub(super) enum PluginSource {
    Unspecified,
    Python,
    Native,
}

pub(super) fn split_plugin_source(plugin: &str) -> Result<(PluginSource, &str), String> {
    match plugin.split_once('!') {
        None => Ok((PluginSource::Unspecified, plugin)),
        Some(("p", name)) => Ok((PluginSource::Python, name)),
        Some(("r", name)) => Ok((PluginSource::Native, name)),
        Some((prefix, _)) => Err(format!(
            "Unknown plugin source '{prefix}!', use 'p!' for Python or 'r!' for Rust plugins."
        )),
    }
}

pub(super) fn python_plugin_file_exists(plugin: &str) -> bool {
    try_get_plugins_path().is_ok_and(|plugins_path| {
        let mut joined_path = plugins_path;
        joined_path.push(plugin);
        joined_path.set_extension("py");
        joined_path.is_file()
    })
}

pub(super) fn try_load_native_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let loaded_plugin = native_plugins::create_native_plugin(py, plugin).unwrap_or_else(|| {
        cold_path();
        Err(PluginLoadError::new_err("No such plugin exists."))
    })?;

    py.get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?
        .cast::<PyDict>()?
        .set_item(plugin, loaded_plugin)
}

pub(super) fn load_plugin_with(
    py: Python<'_>,
    plugin: &str,
    reload_if_loaded: bool,
) -> PyResult<()> {
    let (source, plugin) = split_plugin_source(plugin).map_err(PluginLoadError::new_err)?;

    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let info_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "INFO")))?;
        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    &info_level,
                    intern!(py, ""),
                    -1,
                    intern!(py, "Loading plugin '%s'..."),
                    (plugin,),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "load_plugin"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    if let Some(loaded_plugin) = py
        .get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?
        .cast::<PyDict>()?
        .get_item(plugin)?
        && source != PluginSource::Unspecified
        && native_plugins::is_native_plugin(&loaded_plugin) != (source == PluginSource::Native)
    {
        cold_path();
        let loaded_source = if source == PluginSource::Native {
            "Python"
        } else {
            "Rust"
        };
        return Err(PluginLoadError::new_err(format!(
            "Plugin '{plugin}' is already loaded as a {loaded_source} plugin. Unload it first."
        )));
    }

    let use_native = match source {
        PluginSource::Native => true,
        PluginSource::Python => false,
        PluginSource::Unspecified => {
            native_plugins::has_native_plugin(plugin) && !python_plugin_file_exists(plugin)
        }
    };
    if use_native {
        let loaded_plugins = py
            .get_type::<Plugin>()
            .getattr(intern!(py, "_loaded_plugins"))?;

        if reload_if_loaded && loaded_plugins.cast::<PyDict>()?.contains(plugin)? {
            return reload_plugin(py, plugin);
        }

        return try_load_native_plugin(py, plugin).tap_err(|e| {
            log_exception(py, e);
        });
    }

    try_get_plugins_path().map_or_else(
        |err| {
            cold_path();
            Err(PluginLoadError::new_err(err))
        },
        |plugins_path| {
            let mut joined_path = plugins_path.to_owned();
            joined_path.push(plugin);
            joined_path.set_extension("py");

            if !joined_path.as_path().is_file() {
                cold_path();
                return Err(PluginLoadError::new_err("No such plugin exists."));
            }

            let loaded_plugins = py
                .get_type::<Plugin>()
                .getattr(intern!(py, "_loaded_plugins"))?;

            if reload_if_loaded && loaded_plugins.cast::<PyDict>()?.contains(plugin)? {
                return reload_plugin(py, plugin);
            }

            try_load_plugin(py, plugin, plugins_path.as_path()).tap_err(|e| {
                log_exception(py, e);
            })
        },
    )
}

pub(super) fn try_unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    event_dispatcher(
        py,
        intern!(py, "unload"),
        "could not get access to unload dispatcher",
    )
    .and_then(|unload_dispatcher| {
        UnloadDispatcherMethods::dispatch(
            unload_dispatcher.cast()?,
            PyString::intern(py, plugin).as_any(),
        )
    })?;

    let loaded_plugins = py
        .get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?;

    loaded_plugins
        .cast::<PyDict>()?
        .get_item(plugin)?
        .map_or_else(
            || {
                cold_path();
                let error_msg =
                    format!("Attempted to unload a plugin '{plugin}' that is not loaded.");
                Err(PluginUnloadError::new_err(error_msg))
            },
            |loaded_plugin| {
                loaded_plugin
                    .getattr(intern!(py, "hooks"))?
                    .try_iter()?
                    .flatten()
                    .for_each(|hook| {
                        let _ = hook
                            .cast_into::<PyTuple>()
                            .map_err(PyErr::from)
                            .and_then(|hook_tuple| {
                                let event_name = hook_tuple.get_item(0)?;
                                let event_handler = hook_tuple.get_item(1)?;
                                let event_priority = hook_tuple.get_item(2)?;
                                EVENT_DISPATCHERS
                                    .load()
                                    .as_ref()
                                    .and_then(|event_dispatchers| {
                                        event_dispatchers.bind(py).get_item(&event_name).ok()
                                    })
                                    .map_or_else(
                                        || {
                                            cold_path();
                                            Err(PyAttributeError::new_err(event_name.to_string()))
                                        },
                                        |event_dispatcher| {
                                            let plugin_name =
                                                loaded_plugin.getattr(intern!(py, "name"))?;

                                            event_dispatcher
                                                .call_method1(
                                                    intern!(py, "remove_hook"),
                                                    (plugin_name, event_handler, event_priority),
                                                )
                                                .map(|_| ())
                                        },
                                    )
                            })
                            .tap_err(|err| {
                                log_exception(py, err);
                            });
                    });

                loaded_plugin
                    .getattr(intern!(py, "commands"))?
                    .try_iter()?
                    .flatten()
                    .for_each(|cmd| {
                        let _ = COMMANDS
                            .load()
                            .as_ref()
                            .map_or_else(
                                || {
                                    cold_path();
                                    Err(PyEnvironmentError::new_err(
                                        "could not get access to commands",
                                    ))
                                },
                                |commands| commands.bind(py).remove_command(cmd.cast()?),
                            )
                            .tap_err(|err| {
                                log_exception(py, err);
                            });
                    });

                loaded_plugins.del_item(plugin)
            },
        )
}

#[pyfunction(name = "unload_plugin")]
pub(crate) fn unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let info_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "INFO")))?;

        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    &info_level,
                    intern!(py, ""),
                    -1,
                    intern!(py, "Unloading plugin '%s'..."),
                    (plugin,),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "unload_plugin"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    let loaded_plugins = py
        .get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?;
    if !loaded_plugins.cast::<PyDict>()?.contains(plugin)? {
        cold_path();
        let error_msg = format!("Attempted to unload a plugin '{plugin}' that is not loaded.");
        return Err(PluginUnloadError::new_err(error_msg));
    }

    try_unload_plugin(py, plugin).tap_err(|e| {
        log_exception(py, e);
    })
}

pub(super) fn is_loaded_native_plugin(py: Python<'_>, plugin: &str) -> bool {
    py.get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))
        .and_then(|loaded_plugins| loaded_plugins.get_item(plugin))
        .is_ok_and(|loaded_plugin| native_plugins::is_native_plugin(&loaded_plugin))
}

pub(super) fn try_reload_plugin(py: Python, plugin: &str, target: &str) -> PyResult<()> {
    if !target.starts_with("r!")
        && let Some(loaded_modules) = MODULES.load().as_ref()
        && loaded_modules.bind(py).contains(plugin)?
    {
        loaded_modules
            .bind(py)
            .get_item(plugin)
            .and_then(|loaded_plugin_module| {
                py.import(intern!(py, "importlib"))
                    .and_then(|importlib_module| {
                        importlib_module
                            .call_method1(intern!(py, "reload"), (loaded_plugin_module,))
                    })
                    .and_then(|module| loaded_modules.bind(py).set_item(plugin, module))
            })?;
    };
    load_plugin_with(py, target, false)
}

#[pyfunction(name = "reload_plugin")]
pub(crate) fn reload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let (source, plugin) = split_plugin_source(plugin).map_err(PluginLoadError::new_err)?;
    let reload_target = match source {
        PluginSource::Native => format!("r!{plugin}"),
        PluginSource::Python => format!("p!{plugin}"),
        PluginSource::Unspecified if is_loaded_native_plugin(py, plugin) => format!("r!{plugin}"),
        PluginSource::Unspecified => plugin.to_owned(),
    };
    let _ = unload_plugin(py, plugin);

    try_reload_plugin(py, plugin, &reload_target).tap_err(|e| {
        log_exception(py, e);
    })
}

#[cfg(test)]
mod plugin_source_tests {
    use rstest::rstest;

    use super::{PluginSource, split_plugin_source};

    #[rstest]
    #[case("balance", PluginSource::Unspecified, "balance")]
    #[case("p!balance", PluginSource::Python, "balance")]
    #[case("r!balance", PluginSource::Native, "balance")]
    fn split_plugin_source_recognizes_prefixes(
        #[case] plugin: &str,
        #[case] source: PluginSource,
        #[case] name: &str,
    ) {
        assert_eq!(split_plugin_source(plugin), Ok((source, name)));
    }

    #[rstest]
    #[case("x!balance")]
    #[case("R!balance")]
    #[case("!balance")]
    fn split_plugin_source_rejects_unknown_prefixes(#[case] plugin: &str) {
        assert!(split_plugin_source(plugin).is_err());
    }
}

#[cfg(test)]
mod native_plugin_loading_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{intern, types::PyDict};
    use rstest::rstest;

    use super::{
        EVENT_DISPATCHERS, PluginLoadError, PluginUnloadError, load_plugin, reload_plugin,
        try_load_plugin, unload_plugin,
    };
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                events::{
                    EventDispatcherManager, EventDispatcherManagerMethods, MapDispatcher,
                    UnloadDispatcher,
                },
                prelude::*,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    fn with_native_workshop_engine<F>(times: usize, test: F)
    where
        F: FnOnce(Python<'_>),
    {
        let zmq_stats_enable = c"0";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(zmq_stats_enable.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 0..)
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .with_find_cvar(|cmd| cmd == "qlx_workshopReferences", |_| None, times)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("qlx_workshopReferences"),
                        predicate::eq(""),
                        predicate::eq(Some(0)),
                    )
                    .times(times);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = py.get_type::<Plugin>();
                    let _restore_loaded_plugins =
                        RestoredAttribute::capture(plugin_class.as_any(), "_loaded_plugins");
                    plugin_class
                        .setattr(intern!(py, "_loaded_plugins"), PyDict::new(py))
                        .expect("could not set loaded plugins");

                    let event_dispatchers = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatchers
                        .add_dispatcher(&py.get_type::<MapDispatcher>())
                        .expect("could not add map dispatcher");
                    event_dispatchers
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatchers.unbind().into()));

                    test(py);
                });
            });
    }

    fn loaded_hooks(py: Python<'_>, plugin: &str) -> usize {
        py.get_type::<Plugin>()
            .getattr(intern!(py, "_loaded_plugins"))
            .and_then(|loaded_plugins| loaded_plugins.get_item(plugin))
            .and_then(|plugin| plugin.getattr(intern!(py, "hooks")))
            .and_then(|hooks| hooks.len())
            .expect("could not get the hooks of the loaded plugin")
    }

    #[rstest]
    #[case("r!workshop")]
    #[case("workshop")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_plugin_loads_native_plugin(_pyshinqlx_setup: (), #[case] plugin: &str) {
        with_native_workshop_engine(1, |py| {
            assert!(load_plugin(py, plugin).is_ok());
            assert_eq!(loaded_hooks(py, "workshop"), 1);
        });
    }

    #[rstest]
    #[case("r!no_such_plugin")]
    #[case("x!workshop")]
    #[case("p!workshop")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_plugin_with_unavailable_source_fails(_pyshinqlx_setup: (), #[case] plugin: &str) {
        with_native_workshop_engine(0, |py| {
            let result = load_plugin(py, plugin);
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unload_plugin_unloads_native_plugin(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(1, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            assert!(unload_plugin(py, "workshop").is_ok());
            assert!(
                py.get_type::<Plugin>()
                    .getattr(intern!(py, "_loaded_plugins"))
                    .and_then(|loaded_plugins| loaded_plugins.contains("workshop"))
                    .is_ok_and(|contained| !contained)
            );
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reload_plugin_keeps_native_plugin_native(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(2, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            assert!(reload_plugin(py, "workshop").is_ok());
            assert_eq!(loaded_hooks(py, "workshop"), 1);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_plugin_reloads_an_already_loaded_native_plugin(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(2, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            assert!(load_plugin(py, "r!workshop").is_ok());
            assert_eq!(loaded_hooks(py, "workshop"), 1);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn load_plugin_refuses_another_source_than_the_loaded_one(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(1, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            let result = load_plugin(py, "p!workshop");

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)
                    && err.value(py).to_string()
                        == "Plugin 'workshop' is already loaded as a Rust plugin. Unload it first.")
            );
            assert_eq!(loaded_hooks(py, "workshop"), 1);
        });
    }

    #[rstest]
    #[case("r!workshop")]
    #[case("workshop")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_plugin_accepts_the_native_prefix(_pyshinqlx_setup: (), #[case] plugin: &str) {
        with_native_workshop_engine(2, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            assert!(reload_plugin(py, plugin).is_ok());
            assert_eq!(loaded_hooks(py, "workshop"), 1);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_plugin_with_python_prefix_replaces_a_native_plugin(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(1, |py| {
            load_plugin(py, "r!workshop").expect("could not load native plugin");

            let result = reload_plugin(py, "p!workshop");

            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
            assert!(
                py.get_type::<Plugin>()
                    .getattr(intern!(py, "_loaded_plugins"))
                    .and_then(|loaded_plugins| loaded_plugins.contains("workshop"))
                    .is_ok_and(|contained| !contained)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_plugin_with_unknown_prefix_fails(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(0, |py| {
            let result = reload_plugin(py, "x!workshop");

            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unload_plugin_that_is_not_loaded_fails(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(0, |py| {
            let result = unload_plugin(py, "not_loaded");

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PluginUnloadError>(py)
                    && err.value(py).to_string()
                        == "Attempted to unload a plugin 'not_loaded' that is not loaded.")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reload_plugin_that_cannot_be_loaded_fails(_pyshinqlx_setup: ()) {
        with_native_workshop_engine(0, |py| {
            let result = reload_plugin(py, "no_such_plugin");

            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_load_plugin_requires_a_plugins_directory_name(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = try_load_plugin(py, "any_plugin", std::path::Path::new("/"));

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)
                    && err.value(py).to_string()
                        == "qlx_pluginsPath is not pointing to an existing directory")
            );
        });
    }
}

#[cfg(test)]
mod pyshinqlx_plugins_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;
    use std::{
        fs::{DirBuilder, File},
        io::Write,
        path::PathBuf,
    };

    use pyo3::{exceptions::PyEnvironmentError, intern, types::PyDict};
    use rstest::*;

    use super::{
        DEFAULT_PLUGINS, EVENT_DISPATCHERS, PluginLoadError, PluginUnloadError, load_plugin,
        load_preset_plugins, reload_plugin, unload_plugin,
    };
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::prelude::*,
        },
        prelude::*,
    };

    static TEMP_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[fixture]
    fn plugins_dir() -> String {
        let mut plugins_dir_path = PathBuf::from(TEMP_DIR.as_ref());
        plugins_dir_path.push("shinqlx-plugins");

        if !plugins_dir_path.exists() {
            DirBuilder::new()
                .create(plugins_dir_path.as_path())
                .expect("this should not happen");
        }
        let plugins_dir = plugins_dir_path.to_string_lossy().to_string();

        let mut noclass_plugin_path = PathBuf::from(&plugins_dir);
        noclass_plugin_path.push("noclass_plugin");
        noclass_plugin_path.set_extension("py");

        if !noclass_plugin_path.exists() {
            File::create(noclass_plugin_path).expect("this should not happen");
        }

        let mut nosubclass_plugin_path = PathBuf::from(&plugins_dir);
        nosubclass_plugin_path.push("nosubclass_plugin");
        nosubclass_plugin_path.set_extension("py");

        if !nosubclass_plugin_path.exists() {
            let mut test_plugin =
                File::create(nosubclass_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
class nosubclass_plugin:
    pass
        "#,
                )
                .expect("this should not happen");
        }

        let mut test_plugin_path = PathBuf::from(&plugins_dir);
        test_plugin_path.push("test_plugin");
        test_plugin_path.set_extension("py");

        if !test_plugin_path.exists() {
            let mut test_plugin = File::create(test_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
import shinqlx

class test_plugin(shinqlx.Plugin):
    pass
        "#,
                )
                .expect("this should not happen");
        }

        let mut test_decorated_plugin_path = PathBuf::from(&plugins_dir);
        test_decorated_plugin_path.push("test_decorated_plugin");
        test_decorated_plugin_path.set_extension("py");

        if !test_decorated_plugin_path.exists() {
            let mut test_plugin =
                File::create(test_decorated_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
import shinqlx

class test_decorated_plugin(shinqlx.Plugin):
    @shinqlx.hook("team_switch_attempt")
    def on_team_switch_attempt(self, player, old_team, new_team):
        pass
        "#,
                )
                .expect("this should not happen");
        }

        for plugin in DEFAULT_PLUGINS {
            let mut test_plugin_path = PathBuf::from(&plugins_dir);
            test_plugin_path.push(plugin);
            test_plugin_path.set_extension("py");

            if !test_plugin_path.exists() {
                let mut test_plugin =
                    File::create(test_plugin_path).expect("this should not happen");
                let plugin_code = format!(
                    r#"
import shinqlx

class {plugin}(shinqlx.Plugin):
    pass
                "#
                );
                test_plugin
                    .write_all(plugin_code.as_bytes())
                    .expect("this should not happen");
            }
        }

        let mut test_cmd_hook_plugin_path = PathBuf::from(&plugins_dir);
        test_cmd_hook_plugin_path.push("test_cmd_hook_plugin");
        test_cmd_hook_plugin_path.set_extension("py");

        if !test_cmd_hook_plugin_path.exists() {
            let mut test_plugin =
                File::create(test_cmd_hook_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
import shinqlx

class test_cmd_hook_plugin(shinqlx.Plugin):
    def __init__(self):
        self.add_command("asdf", self.cmd_asdf)

        self.add_hook("game_end", self.handle_game_end)

    def cmd_asdf(self, *args, **kwargs):
        pass

    def handle_game_end(self, *args, **kwargs):
        pass
        "#,
                )
                .expect("this should not happen");
        }

        plugins_dir_path.to_string_lossy().to_string()
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_with_main_engine_missing(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = load_preset_plugins(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugin_with_misconfigured_plugin_path_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 1..)
            .run(|| {
                Python::attach(|py| {
                    let result = load_preset_plugins(py);
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_valid_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_default_plugins(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"DEFAULT, test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_unique_plugins(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"test_plugin, ban, DEFAULT, test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_main_engine_missing(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = load_plugin(py, "not_existing_plugin");
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_misconfigured_plugin_path_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 0..)
            .run(|| {
                Python::attach(|py| {
                    let result = load_plugin(py, "not_existing_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            })
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_not_existing_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "not_existing_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_test_plugin_with_no_class(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "noclass_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_test_plugin_with_no_subclass_of_plugin(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "nosubclass_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_loads_valid_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_registers_decorated_methods(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_stats_enable = c"0";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(zmq_stats_enable.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let event_dispatchers = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatchers
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add team switch attempt dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatchers.unbind().into()));

                    let result = load_plugin(py, "test_decorated_plugin");
                    assert!(result.is_ok());

                    let hooks = py
                        .get_type::<Plugin>()
                        .getattr(intern!(py, "_loaded_plugins"))
                        .and_then(|loaded_plugins| loaded_plugins.get_item("test_decorated_plugin"))
                        .and_then(|plugin| plugin.getattr(intern!(py, "hooks")))
                        .and_then(|hooks| hooks.len())
                        .expect("could not get the hooks of the loaded plugin");
                    assert_eq!(hooks, 1);
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_reloads_already_loaded_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = load_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_failing_unload_does_not_recurse_endlessly(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    py.import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    EVENT_DISPATCHERS.store(None);
                    let result = load_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_a_non_loaded_plugin(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = unload_plugin(py, "non_loaded_plugin");
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginUnloadError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unloading_loaded_plugin_with_no_event_dispatchers(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.clone().unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    EVENT_DISPATCHERS.store(None);

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_that_is_not_stored_in_loaded_plugins(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    py.get_type::<Plugin>()
                        .setattr(intern!(py, "_loaded_plugins"), PyDict::new(py))
                        .expect("this should not happen");

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginUnloadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_unloads_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_unregisters_hooks_and_commands(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_cvar_str = c"1";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(zmq_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_cmd_hook_plugin").expect("this should not happen");

                    let result = unload_plugin(py, "test_cmd_hook_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reload_plugin_reloads_already_loaded_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = reload_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }
}
