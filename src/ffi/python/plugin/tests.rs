use core::borrow::BorrowMut;

use mockall::predicate;
use pretty_assertions::assert_eq;
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
    intern,
    types::{IntoPyDict, PyBool, PyDate, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple},
};
use rstest::rstest;

use super::PluginMethods;
use crate::{
    ffi::{
        c::prelude::*,
        python::{
            BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS,
            RED_TEAM_CHAT_CHANNEL, commands::CommandPriorities,
            events::EventDispatcherManagerMethods, prelude::*, pyshinqlx_test_support::*,
        },
    },
    hooks::mock_hooks::{
        shinqlx_com_printf_context, shinqlx_drop_client_context,
        shinqlx_send_server_command_context,
    },
    prelude::*,
};

#[rstest]
#[cfg_attr(miri, ignore)]
fn plugin_can_be_subclassed_from_python(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let extended_plugin = test_plugin(py);

        let result = extended_plugin.call0();

        assert!(result.is_ok());
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn plugin_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
    let cvar_string = c"1";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    )
                    .expect("this should not happen");

                let result = py
                    .import(intern!(py, "gc"))
                    .and_then(|gc| gc.call_method0(intern!(py, "collect")));
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn str_returns_plugin_typename(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin_instance = test_plugin(py).call0().expect("this should not happen");

        let plugin_str = plugin_instance.str();

        assert!(plugin_str.is_ok_and(|plugin_name| plugin_name == "test_plugin"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_db_when_no_db_type_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_database =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "database");
        let extended_plugin = test_plugin(py);
        py.get_type::<Plugin>()
            .delattr("database")
            .expect("this should not happen");
        let plugin_instance = extended_plugin.call0().expect("this should not happen");

        let result = plugin_instance
            .cast()
            .expect("this should not happen")
            .get_db();

        assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_db_when_no_db_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_database =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "database");
        let extended_plugin = test_plugin(py);
        py.get_type::<Plugin>()
            .setattr("database", py.None())
            .expect("this should not happen");
        let plugin_instance = extended_plugin.call0().expect("this should not happen");

        let result = plugin_instance
            .cast()
            .expect("this should not happen")
            .get_db();

        assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_db_when_db_set_to_redis(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_database =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "database");
        let extended_plugin = test_plugin(py);
        let redis_type = py.get_type::<Redis>();
        py.get_type::<Plugin>()
            .setattr("database", redis_type.as_any())
            .expect("this should not happen");
        let plugin_instance = extended_plugin.call0().expect("this should not happen");

        let result = plugin_instance
            .cast()
            .expect("this should not happen")
            .get_db();

        assert!(result.is_ok_and(|db| db.is_instance(&redis_type).unwrap()));
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn name_property_returns_plugin_typename(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin_instance = test_plugin(py).call0().expect("this should not happen");

        let plugin_str = plugin_instance
            .cast::<Plugin>()
            .expect("this should not happen")
            .get_name();

        assert!(plugin_str.is_ok_and(|plugin_name| plugin_name == "test_plugin"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn plugins_property_returns_loaded_plugins(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_loaded_plugins =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "_loaded_plugins");
        let extended_plugin = test_plugin(py);

        let loaded_plugins = PyDict::new(py);
        loaded_plugins
            .set_item("asdf", "asdfplugin")
            .expect("this should not happen");
        loaded_plugins
            .set_item("qwertz", "qwertzplugin")
            .expect("this should not happen");
        py.get_type::<Plugin>()
            .setattr("_loaded_plugins", &loaded_plugins)
            .expect("this should not happen");
        let plugin_instance = extended_plugin.call0().expect("this should not happen");

        let plugins = plugin_instance
            .cast::<Plugin>()
            .expect("this should not happen")
            .get_plugins();

        assert!(plugins.is_ok_and(|plugins_dict| {
            plugins_dict
                .eq(loaded_plugins)
                .expect("this should not happen")
        }));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn hooks_property_returns_plugin_hooks(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin = Bound::new(
            py,
            Plugin {
                hooks: vec![
                    ("asdf".to_string(), py.None(), 1),
                    ("qwertz".to_string(), py.None(), 0),
                ]
                .into(),
                commands: Default::default(),
                db_instance: py.None().into(),
            },
        )
        .expect("this should not happen");

        let hooks = plugin.get_hooks();
        assert_eq!(hooks.len(), 2);
        let elem1 = hooks.first();
        assert!(elem1.is_some_and(|(hook1, pyobj1, prio1)| hook1 == "asdf"
            && pyobj1.bind(py).is_none()
            && *prio1 == 1));
        let elem2 = hooks.get(1);
        assert!(elem2.is_some_and(|(hook2, pyobj2, prio2)| hook2 == "qwertz"
            && pyobj2.bind(py).is_none()
            && *prio2 == 0));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn commands_property_when_no_commands_exist(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin = Bound::new(
            py,
            Plugin {
                hooks: Default::default(),
                commands: Default::default(),
                db_instance: py.None().into(),
            },
        )
        .expect("this should not happen");

        assert_eq!(plugin.get_commands().len(), 0);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn game_property_when_no_game_running(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin = Bound::new(
            py,
            Plugin {
                hooks: Default::default(),
                commands: Default::default(),
                db_instance: py.None().into(),
            },
        )
        .expect("this should not happen");

        assert!(plugin.get_game().is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn game_property_when_a_game_exists(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_SERVERINFO as u16, "asdf", 1)
        .run(|| {
            Python::attach(|py| {
                let plugin = Bound::new(
                    py,
                    Plugin {
                        hooks: Default::default(),
                        commands: Default::default(),
                        db_instance: py.None().into(),
                    },
                )
                .expect("this should not happen");

                assert!(plugin.get_game().is_some());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_logger(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let logger_type = py
            .import(intern!(py, "logging"))
            .expect("this should not happen")
            .getattr(intern!(py, "Logger"))
            .expect("could not get logging.Logger");

        let plugin_instance = test_plugin(py).call0().expect("this should not happen");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("this should not happen")
            .get_logger();

        assert!(result.is_ok_and(|logger| {
            logger.is_instance(&logger_type).unwrap()
                && logger
                    .getattr(intern!(py, "name"))
                    .expect("could not get logger name")
                    .str()
                    .unwrap()
                    == "shinqlx.test_plugin"
        }),);
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn add_hook_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
    EVENT_DISPATCHERS.store(None);

    Python::attach(|py| {
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_hook(
                "team_switch",
                py.None().bind(py),
                CommandPriorities::PRI_NORMAL as i32,
            );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn add_hook_adds_hook_to_plugin_hooks(_pyshinqlx_setup: ()) {
    let cvar_string = c"0";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                let result = plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    );
                assert!(result.is_ok());
                assert_eq!(
                    plugin_instance
                        .getattr(intern!(py, "hooks"))
                        .expect("could not get hooks")
                        .cast::<PyList>()
                        .expect("could not downcast to list")
                        .len(),
                    1
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn add_hook_adds_hook_to_event_dispatchers(_pyshinqlx_setup: ()) {
    let cvar_string = c"0";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                let result = plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    );

                assert!(result.is_ok());
                assert!(
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .expect("could not get access to event dispatchers")
                        .getattr(py, intern!(py, "_dispatchers"))
                        .expect("could not get dispatchers")
                        .cast_bound::<PyDict>(py)
                        .expect("could not downcast to dict")
                        .get_item(intern!(py, "team_switch_attempt"))
                        .expect("could not get team switch attempt dispatcher")
                        .is_some_and(|team_switch_attempt_dispatcher| {
                            team_switch_attempt_dispatcher
                                .getattr(intern!(py, "plugins"))
                                .expect("could not get plugins")
                                .cast::<PyDict>()
                                .expect("could not downcast to dict")
                                .get_item(intern!(py, "test_plugin"))
                                .is_ok_and(|opt_plugin| {
                                    opt_plugin.is_some_and(|plugin| {
                                        plugin
                                            .get_item(CommandPriorities::PRI_NORMAL as i32)
                                            .is_ok_and(|normal_hooks| {
                                                normal_hooks.len().is_ok_and(|len| len == 1)
                                            })
                                    })
                                })
                        })
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_hook_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        EVENT_DISPATCHERS.store(None);
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_hook(
                "team_switch",
                py.None().bind(py),
                CommandPriorities::PRI_NORMAL as i32,
            );

        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_hook_removes_hook_from_event_dispatchers(_pyshinqlx_setup: ()) {
    let cvar_string = c"1";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    )
                    .expect("could not add command");

                let result = plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .remove_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    );

                assert!(result.is_ok());
                assert!(
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .expect("could not get access to event dispatchers")
                        .getattr(py, intern!(py, "_dispatchers"))
                        .expect("could not get dispatchers")
                        .cast_bound::<PyDict>(py)
                        .expect("could not downcast to dict")
                        .get_item(intern!(py, "team_switch_attempt"))
                        .expect("could not get team switch attempt dispatcher")
                        .is_some_and(|team_switch_attempt_dispatcher| {
                            team_switch_attempt_dispatcher
                                .getattr(intern!(py, "plugins"))
                                .expect("could not get plugins")
                                .cast::<PyDict>()
                                .expect("could not downcast to dict")
                                .get_item(intern!(py, "test_plugin"))
                                .is_ok_and(|opt_plugin| {
                                    opt_plugin.is_some_and(|plugin| {
                                        plugin
                                            .get_item(CommandPriorities::PRI_NORMAL as i32)
                                            .is_ok_and(|normal_hooks| {
                                                normal_hooks.len().is_ok_and(|len| len == 0)
                                            })
                                    })
                                })
                        })
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_hook_removes_hook_from_plugin_instance(_pyshinqlx_setup: ()) {
    let cvar_string = c"0";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    )
                    .expect("could not add command");

                let result = plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .remove_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    );

                assert!(result.is_ok());
                assert!(
                    plugin_instance
                        .getattr(intern!(py, "hooks"))
                        .expect("could not get hooks")
                        .cast::<PyList>()
                        .expect("could not downcast to list")
                        .is_empty()
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_hook_removes_hook_when_other_hook_with_different_priority_exists(_pyshinqlx_setup: ()) {
    let cvar_string = c"0";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1..,
        )
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let plugin_instance = test_plugin(py)
                    .call0()
                    .expect("could not create plugin instance");

                plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    )
                    .expect("could not add command");

                plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .add_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_HIGH as i32,
                    )
                    .expect("could not add command");

                let result = plugin_instance
                    .cast::<Plugin>()
                    .expect("could not downcast instance to plugin")
                    .remove_hook(
                        "team_switch_attempt",
                        py.None().bind(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    );

                assert!(result.is_ok());
                assert_eq!(
                    plugin_instance
                        .getattr(intern!(py, "hooks"))
                        .expect("could not get hooks")
                        .cast::<PyList>()
                        .expect("could not downcast to list")
                        .len(),
                    1
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn add_command_adds_a_new_command(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            );

        assert!(result.is_ok());
        assert!(
            COMMANDS
                .load()
                .as_ref()
                .expect("could not get access to commands")
                .getattr(py, intern!(py, "commands"))
                .expect("could not get commands")
                .cast_bound::<PyList>(py)
                .expect("could not downcast to list")
                .get_item(0)
                .expect("could not get first command")
                .getattr(intern!(py, "name"))
                .expect("could not get name attr")
                .get_item(0)
                .expect("could not get first name of command")
                .str()
                .is_ok_and(|value| value == "slap")
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn add_command_stores_command_in_plugin(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            );

        assert!(result.is_ok());
        assert_eq!(
            plugin_instance
                .getattr(intern!(py, "commands"))
                .expect("could not get commands")
                .cast::<PyList>()
                .expect("could not downcast to list")
                .len(),
            1
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_removes_command(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_command(PyString::intern(py, "slap").as_any(), &command_handler);

        assert!(result.is_ok());
        assert!(
            COMMANDS
                .load()
                .as_ref()
                .expect("could not get access to commands")
                .getattr(py, intern!(py, "commands"))
                .expect("could not get commands")
                .cast_bound::<PyList>(py)
                .expect("could not downcast to list")
                .is_empty()
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_removes_command_with_other_cmd_left_in_place(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyTuple::new(py, ["slay", "asdf"])
                    .expect("this should not happen")
                    .as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_command(
                PyTuple::new(py, ["slay", "asdf"])
                    .expect("this should not happen")
                    .as_any(),
                &command_handler,
            );

        assert!(result.is_ok());
        run_all_frame_tasks(py).expect("could not run all frame tasks");
        assert_eq!(
            COMMANDS
                .load()
                .as_ref()
                .expect("could not get access to commands")
                .getattr(py, intern!(py, "commands"))
                .expect("could not get commands")
                .cast_bound::<PyList>(py)
                .expect("could not downcast to list")
                .len(),
            1
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_for_list_of_command_names(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyTuple::new(py, ["slay", "asdf"])
                    .expect("this should not happen")
                    .as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_command(
                PyTuple::new(py, ["slay", "asdf"])
                    .expect("this should not happen")
                    .as_any(),
                &command_handler,
            );

        assert!(result.is_ok());
        run_all_frame_tasks(py).expect("could not run all frame tasks");
        assert!(
            COMMANDS
                .load()
                .as_ref()
                .expect("could not get access to commands")
                .getattr(py, intern!(py, "commands"))
                .expect("could not get commands")
                .cast_bound::<PyList>(py)
                .expect("could not downcast to list")
                .is_empty(),
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_removes_command_in_plugin_instance(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        let command_invoker = CommandInvoker::py_new();
        COMMANDS.store(Some(
            Py::new(py, command_invoker)
                .expect("could not create command invoker in python")
                .into(),
        ));

        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_command(PyString::intern(py, "slap").as_any(), &command_handler);

        assert!(result.is_ok());
        assert!(
            plugin_instance
                .getattr(intern!(py, "commands"))
                .expect("could not get commands")
                .cast::<PyList>()
                .expect("could not downcast to list")
                .is_empty(),
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "sv_maxclients", None);
        assert!(result.is_ok_and(|value| value.is_none()));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_when_cvar_not_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "asdf", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "asdf", None);
                assert!(result.expect("result was not OK").is_none());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_when_cvar_is_found(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "sv_maxclients", None);
                assert!(
                    result
                        .expect("result was not OK")
                        .extract::<String>()
                        .is_ok_and(|value| value == "16")
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_str(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyString>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(
                    result
                        .expect("result was not OK")
                        .extract::<String>()
                        .is_ok_and(|value| value == "16")
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_str_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyString>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(result.expect("result was not OK").is_none());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_int(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyInt>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(
                    result
                        .expect("result was not OK")
                        .extract::<i32>()
                        .is_ok_and(|value| value == 16)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_int_when_cvar_cannot_be_converted_to_int(_pyshinqlx_setup: ()) {
    let cvar_string = c"asdf";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyInt>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_int_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyInt>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(result.expect("result was not OK").is_none());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_float(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyFloat>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(
                    result
                        .expect("result was not OK")
                        .extract::<f64>()
                        .is_ok_and(|value| value == 16.0)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_float_when_cvar_cannot_be_converted_to_float(_pyshinqlx_setup: ()) {
    let cvar_string = c"asdf";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyFloat>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_float_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyFloat>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );
                assert!(result.expect("result was not OK").is_none());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_bool(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyBool>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");
                assert!(
                    result.is_instance_of::<PyBool>()
                        && result.extract::<bool>().is_ok_and(|value| value)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_bool_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyBool>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");

                assert!(
                    result.is_instance_of::<PyBool>()
                        && result.extract::<bool>().is_ok_and(|value| !value)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_bool_when_cvar_cannot_be_converted_to_int(_pyshinqlx_setup: ()) {
    let cvar_string = c"asdf";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyBool>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );

                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_list(_pyshinqlx_setup: ()) {
    let cvar_string = c"2, 4, 6, 8, 10";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyList>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");
                assert!(
                    result.is_instance_of::<PyList>()
                        && result.cast::<PyList>().is_ok_and(|value| value.len() == 5)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_list_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyList>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");

                assert!(
                    result.is_instance_of::<PyList>()
                        && result.cast::<PyList>().is_ok_and(|value| value.is_empty())
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_set(_pyshinqlx_setup: ()) {
    let cvar_string = c"2, 4, 6, 8, 10";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PySet>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");
                assert!(
                    result.is_instance_of::<PySet>()
                        && result.cast::<PySet>().is_ok_and(|value| value.len() == 5)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_set_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PySet>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");

                assert!(
                    result.is_instance_of::<PySet>()
                        && result.cast::<PySet>().is_ok_and(|value| value.is_empty())
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_converts_to_tuple(_pyshinqlx_setup: ()) {
    let cvar_string = c"2, 4, 6, 8, 10";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyTuple>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");
                assert!(
                    result.is_instance_of::<PyTuple>()
                        && result.cast::<PyTuple>().is_ok_and(|value| value.len() == 5)
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_tuple_when_no_cvar_found(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyTuple>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                )
                .expect("result was not OK");

                assert!(
                    result.is_instance_of::<PyTuple>()
                        && result.cast::<PyTuple>().is_ok_and(|value| value.is_empty())
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_cvar_with_invalid_type_conversion(_pyshinqlx_setup: ()) {
    let cvar_string = c"16";
    let mut raw_cvar = CVarBuilder::default()
        .string(cvar_string.as_ptr().cast_mut())
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            0,
        )
        .run(|| {
            Python::attach(|py| {
                let py_str_type = py.get_type::<PyDate>();
                let result = Plugin::get_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    Some(py_str_type.unbind()),
                );

                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::set_cvar(
            &py.get_type::<Plugin>(),
            "sv_maxclients",
            PyString::intern(py, "64").as_any(),
            0,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_for_not_existing_cvar(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_get_cvar()
                .with(
                    predicate::eq("sv_maxclients"),
                    predicate::eq("64"),
                    predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
                )
                .times(1);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyString::intern(py, "64").as_any(),
                    cvar_flags::CVAR_ROM as i32,
                )
            });
            assert_eq!(result.expect("result was not OK"), true);
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_for_already_existing_cvar(_pyshinqlx_setup: ()) {
    let mut raw_cvar = CVarBuilder::default()
        .build()
        .expect("this should not happen");

    MockEngineBuilder::default()
        .with_execute_console_command(r#"sv_maxclients "64""#, 1)
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyString::intern(py, "64").as_any(),
                    cvar_flags::CVAR_ROM as i32,
                )
            });
            assert_eq!(result.expect("result was not OK"), false);
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_limit_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::set_cvar_limit(
            &py.get_type::<Plugin>(),
            "sv_maxclients",
            PyInt::new(py, 64i32).as_any(),
            PyInt::new(py, 1i32).as_any(),
            PyInt::new(py, 64i32).as_any(),
            0,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_limit_forwards_parameters_to_main_engine_call(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_set_cvar_limit()
                .with(
                    predicate::eq("sv_maxclients"),
                    predicate::eq("64"),
                    predicate::eq("1"),
                    predicate::eq("64"),
                    predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
                )
                .times(1);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar_limit(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyInt::new(py, 64i32).as_any(),
                    PyInt::new(py, 1i32).as_any(),
                    PyInt::new(py, 64i32).as_any(),
                    cvar_flags::CVAR_CHEAT as i32,
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_once_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::set_cvar_once(
            &py.get_type::<Plugin>(),
            "sv_maxclients",
            PyString::intern(py, "64").as_any(),
            0,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_once_for_not_existing_cvar(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_get_cvar()
                .with(
                    predicate::eq("sv_maxclients"),
                    predicate::eq("64"),
                    predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
                )
                .times(1);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar_once(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyInt::new(py, 64i32).as_any(),
                    cvar_flags::CVAR_ROM as i32,
                )
            })
            .unwrap();
            assert_eq!(result, true);
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
    let mut raw_cvar = CVarBuilder::default().build().unwrap();

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .configure(|mock_engine| {
            mock_engine.expect_get_cvar().times(0);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar_once(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyString::intern(py, "64").as_any(),
                    cvar_flags::CVAR_ROM as i32,
                )
            })
            .unwrap();
            assert_eq!(result, false);
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_limit_once_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::set_cvar_limit_once(
            &py.get_type::<Plugin>(),
            "sv_maxclients",
            PyString::intern(py, "64").as_any(),
            PyString::intern(py, "1").as_any(),
            PyString::intern(py, "64").as_any(),
            0,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_limit_once_when_no_previous_value_is_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1..9)
        .configure(|mock_engine| {
            mock_engine
                .expect_set_cvar_limit()
                .with(
                    predicate::eq("sv_maxclients"),
                    predicate::eq("64"),
                    predicate::eq("1"),
                    predicate::eq("64"),
                    predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
                )
                .times(1);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar_limit_once(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyString::intern(py, "64").as_any(),
                    PyString::intern(py, "1").as_any(),
                    PyString::intern(py, "64").as_any(),
                    cvar_flags::CVAR_CHEAT as i32,
                )
            });
            assert!(result.is_ok_and(|value| value));
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvar_limit_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
    let mut raw_cvar = CVarBuilder::default().build().unwrap();

    MockEngineBuilder::default()
        .with_find_cvar(
            |cmd| cmd == "sv_maxclients",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            1,
        )
        .configure(|mock_engine| {
            mock_engine.expect_set_cvar_limit().times(0);
        })
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::set_cvar_limit_once(
                    &py.get_type::<Plugin>(),
                    "sv_maxclients",
                    PyString::intern(py, "64").as_any(),
                    PyString::intern(py, "1").as_any(),
                    PyString::intern(py, "64").as_any(),
                    cvar_flags::CVAR_ROM as i32,
                )
            })
            .unwrap();
            assert_eq!(result, false);
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn all_players_for_existing_clients(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(0))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    client_try_from_ctx
        .expect()
        .with(predicate::eq(1))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let game_entity_try_from_ctx = MockGameEntity::from_context();
    game_entity_try_from_ctx.expect().returning(|_client_id| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_player_name()
            .returning(|| "Mocked Player".to_string());
        mock_game_entity
            .expect_get_team()
            .returning(|| team_t::TEAM_RED);
        mock_game_entity
            .expect_get_privileges()
            .returning(|| privileges_t::PRIV_NONE);
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(3).run(|| {
        let all_players = Python::attach(|py| Plugin::players(&py.get_type::<Plugin>()));
        assert_eq!(
            all_players.expect("result was not ok"),
            vec![
                Player {
                    id: 0,
                    player_info: PlayerInfo {
                        client_id: 0,
                        name: "Mocked Player".into(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".into(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        ..default_test_player_info()
                    }
                    .into(),
                    user_info: "asdf".into(),
                    steam_id: 1234,
                    name: "Mocked Player".to_string().into(),
                    ..default_test_player()
                },
                Player {
                    id: 2,
                    player_info: PlayerInfo {
                        client_id: 2,
                        name: "Mocked Player".into(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".into(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        ..default_test_player_info()
                    }
                    .into(),
                    user_info: "asdf".into(),
                    steam_id: 1234,
                    name: "Mocked Player".to_string().into(),
                    ..default_test_player()
                },
            ]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_for_provided_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::player(
            &py.get_type::<Plugin>(),
            Bound::new(py, default_test_player())
                .expect("this should not happen")
                .as_any(),
            None,
        );
        assert!(
            result
                .expect("result was not ok")
                .is_some_and(|result_player| default_test_player() == result_player)
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn player_for_player_id(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(42))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_player_name(|| "Mocked Player".to_string(), 1..)
        .with_team(|| team_t::TEAM_RED, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(42), || {
            Python::attach(|py| {
                let result = Plugin::player(
                    &py.get_type::<Plugin>(),
                    PyInt::new(py, 42i32).as_any(),
                    None,
                );
                assert!(result.expect("result was not ok").is_some_and(|player| {
                    player
                        == Player {
                            id: 42,
                            name: "Mocked Player".to_string().into(),
                            steam_id: 1234,
                            user_info: "asdf".into(),
                            player_info: PlayerInfo {
                                client_id: 42,
                                name: "Mocked Player".into(),
                                team: team_t::TEAM_RED as i32,
                                steam_id: 1234,
                                userinfo: "asdf".into(),
                                connection_state: clientState_t::CS_ACTIVE as i32,
                                ..default_test_player_info()
                            }
                            .into(),
                            ..default_test_player()
                        }
                }));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_for_provided_steam_id_from_player_list(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Player {
            steam_id: 1234,
            player_info: PlayerInfo {
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let result = Plugin::player(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 1234i32).as_any(),
            Some(vec![player.to_owned()]),
        );
        assert!(
            result
                .expect("result was not ok")
                .is_some_and(|result_player| result_player == player)
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_for_provided_steam_id_not_in_provided_player_list(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::player(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 4321i32).as_any(),
            Some(vec![default_test_player()]),
        );
        assert!(result.expect("result was not ok").is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_for_provided_name_from_player_list(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Player {
            name: "Mocked Player".to_string().into(),
            player_info: PlayerInfo {
                name: "Mocked Player".into(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let result = Plugin::player(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "Mocked Player").as_any(),
            Some(vec![player.to_owned()]),
        );
        assert!(
            result
                .expect("result was not ok")
                .is_some_and(|result_player| result_player == player)
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_for_provided_name_not_in_provided_player_list(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::player(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "disconnected").as_any(),
            Some(vec![default_test_player()]),
        );
        assert!(result.expect("result was not ok").is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn msg_for_invalid_channel(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::msg(
            &py.get_type::<Plugin>(),
            "asdf",
            Some(PyString::intern(py, "asdf").into_any()),
            None,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_default_channel(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "print \"asdf\n\"\n")
        .times(1);

    Python::attach(|py| {
        let channel = Bound::new(
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
        .expect("creating new chat channel failed.");
        CHAT_CHANNEL.store(Some(channel.unbind().into()));

        let result = Plugin::msg(&py.get_type::<Plugin>(), "asdf", None, None);
        assert!(result.is_ok());
        run_all_frame_tasks(py).expect("running frame tasks returned an error");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_chat_channel_with_kwargs(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "print \"asdf qwertz\n\"\n")
        .times(1);

    Python::attach(|py| {
        let channel = Bound::new(
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
        .expect("creating new chat channel failed.");
        CHAT_CHANNEL.store(Some(channel.unbind().into()));

        let result = Plugin::msg(
            &py.get_type::<Plugin>(),
            "asdf qwertz",
            Some(PyString::intern(py, "chat").into_any()),
            Some(
                &[
                    ("limit", PyInt::new(py, 23i32).as_any()),
                    ("delimiter", PyString::intern(py, "_").as_any()),
                ]
                .into_py_dict(py)
                .expect("this should not happen"),
            ),
        );
        assert!(result.is_ok());
        run_all_frame_tasks(py).expect("running frame tasks returned an error");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_red_team_chat_channel(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .withf(|client_id| (0..8).contains(client_id))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let game_entity_try_from_ctx = MockGameEntity::from_context();
    game_entity_try_from_ctx
        .expect()
        .withf(|client_id| (0..8).contains(client_id))
        .returning(|client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".to_string());
            mock_game_entity
                .expect_get_team()
                .returning(move || match client_id {
                    0 | 4 => team_t::TEAM_FREE,
                    1 | 5 => team_t::TEAM_RED,
                    2 | 6 => team_t::TEAM_BLUE,
                    _ => team_t::TEAM_SPECTATOR,
                });
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
        .times(2);

    MockEngineBuilder::default().with_max_clients(8).run(|| {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                TeamChatChannel::py_new(
                    py,
                    "red",
                    "red_team_chat",
                    "print \"{}\n\"\n",
                    py.None().bind(py),
                    None,
                ),
            )
            .expect("creating new chat channel failed.");
            RED_TEAM_CHAT_CHANNEL.store(Some(channel.unbind().into()));

            let result = Plugin::msg(
                &py.get_type::<Plugin>(),
                "asdf qwertz",
                Some(PyString::intern(py, "red_team_chat").into_any()),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_blue_team_chat_channel(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .withf(|client_id| (0..8).contains(client_id))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let game_entity_try_from_ctx = MockGameEntity::from_context();
    game_entity_try_from_ctx
        .expect()
        .withf(|client_id| (0..8).contains(client_id))
        .returning(|client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".to_string());
            mock_game_entity
                .expect_get_team()
                .returning(move || match client_id {
                    0 | 4 => team_t::TEAM_FREE,
                    1 | 5 => team_t::TEAM_RED,
                    2 | 6 => team_t::TEAM_BLUE,
                    _ => team_t::TEAM_SPECTATOR,
                });
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
        .times(2);

    MockEngineBuilder::default().with_max_clients(8).run(|| {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                TeamChatChannel::py_new(
                    py,
                    "blue",
                    "blue_team_chat",
                    "print \"{}\n\"\n",
                    py.None().bind(py),
                    None,
                ),
            )
            .expect("creating new chat channel failed.");
            BLUE_TEAM_CHAT_CHANNEL.store(Some(channel.unbind().into()));

            let result = Plugin::msg(
                &py.get_type::<Plugin>(),
                "asdf qwertz",
                Some(PyString::intern(py, "blue_team_chat").into_any()),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_console_channel(_pyshinqlx_setup: ()) {
    let com_printf_ctx = shinqlx_com_printf_context();
    com_printf_ctx
        .expect()
        .withf(|msg| msg == "asdf\n")
        .times(1);

    Python::attach(|py| {
        let console_channel = Bound::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
            .expect("creating new console channel failed.");
        CONSOLE_CHANNEL.store(Some(console_channel.unbind().into()));

        let result = Plugin::msg(
            &py.get_type::<Plugin>(),
            "asdf",
            Some(PyString::intern(py, "console").into_any()),
            None,
        );
        assert!(result.is_ok());
        run_all_frame_tasks(py).expect("running frame tasks returned an error");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn msg_for_provided_channel(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                TellChannel::py_new(py, &default_test_player(), py.None().bind(py), None),
            )
            .expect("could not create tell channel");

            let result = Plugin::msg(
                &py.get_type::<Plugin>(),
                "asdf qwertz",
                Some(channel.into_any()),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn console_prints_to_console(_pyshinqlx_setup: ()) {
    let com_printf_ctx = shinqlx_com_printf_context();
    com_printf_ctx
        .expect()
        .with(predicate::eq("asdf\n"))
        .times(1);

    Python::attach(|py| {
        let result = Plugin::console(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "asdf").as_any(),
        );
        assert!(result.is_ok());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn clean_text_cleans_text_from_color_tags(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::clean_text(
            &py.get_type::<Plugin>(),
            &PyString::new(py, "^0a^1b^2c^3d^4e^5f^6g^7h^8i^9j"),
        );
        assert!(result.is_ok_and(|cleaned| cleaned == "abcdefgh^8i^9j"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn clean_text_returns_text_without_color_tags_as_it_is(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let text = PyString::new(py, "Player ^ name");
        let result = Plugin::clean_text(&py.get_type::<Plugin>(), &text);
        assert!(result.is_ok_and(|cleaned| cleaned.is(&text)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn clean_text_cleans_texts_longer_than_the_stack_buffer(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let text = "^1ab".repeat(200);
        let result = Plugin::clean_text(&py.get_type::<Plugin>(), &PyString::new(py, &text));
        assert!(result.is_ok_and(|cleaned| cleaned == *"ab".repeat(200)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn clean_text_keeps_multibyte_characters(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::clean_text(&py.get_type::<Plugin>(), &PyString::new(py, "ä^1ö^^2ü€^"));
        assert!(result.is_ok_and(|cleaned| cleaned == "äö^ü€^"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn colored_name_for_provided_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Player {
            name: "Mocked Player".to_string().into(),
            ..default_test_player()
        };
        let result = Plugin::colored_name(
            &py.get_type::<Plugin>(),
            Bound::new(py, player)
                .expect("this should not happen")
                .as_any(),
            None,
        );
        assert_eq!(result.expect("result was none"), "Mocked Player");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn colored_name_for_player_in_provided_playerlist(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Player {
            name: "Mocked Player".to_string().into(),
            ..default_test_player()
        };
        let result = Plugin::colored_name(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "Mocked Player").as_any(),
            Some(vec![player]),
        );
        assert_eq!(result.expect("result was none"), "Mocked Player");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn colored_name_for_player_with_colored_name_in_provided_playerlist(_pyshinqlx_setup: ()) {
    let player = Player {
        player_info: PlayerInfo {
            name: "^1Mocked ^4Player".to_string(),
            ..default_test_player_info()
        }
        .into(),
        name: "^1Mocked ^4Player".to_string().into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::colored_name(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "Mocked Player").as_any(),
            Some(vec![player]),
        );
        assert_eq!(result.expect("result was none"), "^1Mocked ^4Player");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn colored_name_for_unavailable_player(_pyshinqlx_setup: ()) {
    let player = Player {
        player_info: PlayerInfo {
            name: "^1Mocked ^4Player".to_string(),
            ..default_test_player_info()
        }
        .into(),
        name: "^1Mocked ^4Player".to_string().into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::colored_name(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "disconnected Player").as_any(),
            Some(vec![player]),
        );
        assert!(result.is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_integer_in_client_id_range(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 42i32).as_any(),
            None,
        );
        assert!(result.is_some_and(|value| value == 42));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_player(_pyshinqlx_setup: ()) {
    let player = Player {
        id: 21,
        player_info: PlayerInfo {
            client_id: 21,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            Bound::new(py, player)
                .expect("this should not happen")
                .as_any(),
            None,
        );
        assert!(result.is_some_and(|value| value == 21));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_steam_id(_pyshinqlx_setup: ()) {
    let player = Player {
        id: 21,
        steam_id: 1234,
        player_info: PlayerInfo {
            client_id: 21,
            steam_id: 1234,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 1234i64).as_any(),
            Some(vec![player]),
        );
        assert!(result.is_some_and(|value| value == 21));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_steam_id_not_in_player_list(_pyshinqlx_setup: ()) {
    let player = Player {
        steam_id: 1234,
        player_info: PlayerInfo {
            steam_id: 1234,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 4321i64).as_any(),
            Some(vec![player]),
        );
        assert!(result.is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_player_name(_pyshinqlx_setup: ()) {
    let player = Player {
        id: 21,
        name: "Mocked Player".to_string().into(),
        player_info: PlayerInfo {
            client_id: 21,
            name: "Mocked Player".to_string(),
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "Mocked Player").as_any(),
            Some(vec![player]),
        );
        assert!(result.is_some_and(|value| value == 21));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_player_name_not_in_player_list(_pyshinqlx_setup: ()) {
    let player = Player {
        player_info: PlayerInfo {
            name: "Mocked Player".to_string(),
            ..default_test_player_info()
        }
        .into(),
        name: "Mocked Player".to_string().into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "UnknownPlayer").as_any(),
            Some(vec![player]),
        );
        assert!(result.is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn client_id_for_unsupported_search_criteria(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::client_id(
            &py.get_type::<Plugin>(),
            PyFloat::new(py, 3.42f64).as_any(),
            Some(vec![default_test_player()]),
        );
        assert!(result.is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn find_player_with_empty_str_returns_player_list(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 21,
        steam_id: 1234,
        player_info: PlayerInfo {
            client_id: 21,
            steam_id: 1234,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 0,
        steam_id: 1235,
        player_info: PlayerInfo {
            client_id: 0,
            steam_id: 1235,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    Python::attach(|py| {
        let result = Plugin::find_player(
            &py.get_type::<Plugin>(),
            "",
            Some(vec![player1.to_owned(), player2.to_owned()]),
        );
        assert_eq!(result, vec![player1, player2]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn find_players_by_matching_provided_names(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 21,
        name: "^1Found ^4Player".to_string().into(),
        steam_id: 1234,
        player_info: PlayerInfo {
            client_id: 21,
            name: "^1Found ^4Player".to_string(),
            steam_id: 1234,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 0,
        name: "non-matching Player".to_string().into(),
        steam_id: 1235,
        player_info: PlayerInfo {
            client_id: 0,
            name: "non-matching Player".to_string(),
            steam_id: 1235,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player3 = Player {
        id: 5,
        name: "found Player".to_string().into(),
        steam_id: 1236,
        player_info: PlayerInfo {
            client_id: 5,
            name: "found Player".to_string(),
            steam_id: 1236,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    Python::attach(|py| {
        let result = Plugin::find_player(
            &py.get_type::<Plugin>(),
            "foU^3nd",
            Some(vec![
                player1.to_owned(),
                player2.to_owned(),
                player3.to_owned(),
            ]),
        );
        assert_eq!(result, vec![player1, player3]);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn find_players_when_no_player_matches(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 21,
        name: "^1non-matching ^4Player".to_string().into(),
        steam_id: 1234,
        player_info: PlayerInfo {
            client_id: 21,
            name: "^1non-matching ^4Player".to_string(),
            steam_id: 1234,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 0,
        name: "non-matching Player".to_string().into(),
        steam_id: 1235,
        player_info: PlayerInfo {
            client_id: 0,
            name: "non-matching Player".to_string(),
            steam_id: 1235,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player3 = Player {
        id: 5,
        name: "non-matching Player".to_string().into(),
        steam_id: 1236,
        player_info: PlayerInfo {
            client_id: 5,
            name: "non-matching Player".to_string(),
            steam_id: 1236,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    Python::attach(|py| {
        let result = Plugin::find_player(
            &py.get_type::<Plugin>(),
            "no-such-player",
            Some(vec![player1, player2, player3]),
        );
        assert!(result.is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn teams_when_no_player_in_player_list(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::teams(&py.get_type::<Plugin>(), Some(vec![]));
        assert!(
            result
                .expect("result was not ok")
                .eq([
                    (PyString::intern(py, "free").into_any(), PyList::empty(py)),
                    (PyString::intern(py, "red").into_any(), PyList::empty(py)),
                    (PyString::intern(py, "blue").into_any(), PyList::empty(py)),
                    (
                        PyString::intern(py, "spectator").into_any(),
                        PyList::empty(py)
                    )
                ]
                .into_py_dict(py)
                .expect("this should not happen"))
                .expect("comparison was not ok"),
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn teams_when_every_team_has_one_player(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 0,
        steam_id: 1234,
        player_info: PlayerInfo {
            client_id: 0,
            steam_id: 1234,
            team: team_t::TEAM_FREE as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 1,
        steam_id: 1235,
        player_info: PlayerInfo {
            client_id: 1,
            steam_id: 1235,
            team: team_t::TEAM_RED as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player3 = Player {
        id: 2,
        steam_id: 1236,
        player_info: PlayerInfo {
            client_id: 2,
            steam_id: 1236,
            team: team_t::TEAM_BLUE as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player4 = Player {
        id: 3,
        steam_id: 1237,
        player_info: PlayerInfo {
            client_id: 3,
            steam_id: 1234,
            team: team_t::TEAM_SPECTATOR as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    Python::attach(|py| {
        let result = Plugin::teams(
            &py.get_type::<Plugin>(),
            Some(vec![
                player4.to_owned(),
                player3.to_owned(),
                player2.to_owned(),
                player1.to_owned(),
            ]),
        );
        assert!(
            result
                .expect("result was not ok")
                .eq([
                    (
                        PyString::intern(py, "free").into_any(),
                        vec![player1]
                            .into_bound_py_any(py)
                            .expect("this should not happen"),
                    ),
                    (
                        PyString::intern(py, "red").into_any(),
                        vec![player2]
                            .into_bound_py_any(py)
                            .expect("this should not happen")
                    ),
                    (
                        PyString::intern(py, "blue").into_any(),
                        vec![player3]
                            .into_bound_py_any(py)
                            .expect("this should not happen")
                    ),
                    (
                        PyString::intern(py, "spectator").into_any(),
                        vec![player4]
                            .into_bound_py_any(py)
                            .expect("this should not happen")
                    )
                ]
                .into_py_dict(py)
                .expect("this should not happen"))
                .expect("comparison was not ok")
        );
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn center_print_to_all_players_sends_center_print_server_command(_pyshinqlx_setup: ()) {
    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|recipients, cmd| recipients.is_none() && cmd == "cp \"asdf\"")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        let result =
            Python::attach(|py| Plugin::center_print(&py.get_type::<Plugin>(), "asdf", None));
        assert!(result.is_ok());
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn center_print_to_paetticular_player_sends_center_print_server_command(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|recipients, cmd| recipients.is_some() && cmd == "cp \"asdf\"")
        .times(1);

    let player = default_test_player();

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        let result = Python::attach(|py| {
            Plugin::center_print(
                &py.get_type::<Plugin>(),
                "asdf",
                Some(Bound::new(py, player)?.into_any()),
            )
        });
        assert!(result.is_ok());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn tell_sends_msg_to_player(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "print \"asdf\n\"\n");

    MockGameEntityBuilder::default()
        .with_player_name(|| "Mocked Player".to_string(), 1..)
        .with_team(|| team_t::TEAM_RED, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let result = Plugin::tell(
                        &py.get_type::<Plugin>(),
                        "asdf",
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                        None,
                    );
                    assert!(result.is_ok());
                    run_all_frame_tasks(py).expect("running frame tasks returned an error");
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn is_vote_active_when_configstring_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_STRING as u16, "vote is active", 1)
        .run(|| {
            Python::attach(|py| {
                assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), true);
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn is_vote_active_when_configstring_empty(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
        .run(|| {
            Python::attach(|py| {
                assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), false);
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn is_vote_active_when_main_engine_not_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), false);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_when_main_engine_not_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
        assert!(result.is_ok_and(|value| value.is_none()))
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_when_yes_votes_are_empty(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_YES as u16, "", 1)
        .with_get_configstring(CS_VOTE_NO as u16, "42", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                assert!(result.is_ok_and(|value| value.is_none()));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_when_no_votes_are_empty(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
        .with_get_configstring(CS_VOTE_NO as u16, "", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                assert!(result.is_ok_and(|value| value.is_none()));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_with_proper_vote_counts(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
        .with_get_configstring(CS_VOTE_NO as u16, "21", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                assert!(
                    result
                        .expect("result was not ok")
                        .eq(PyTuple::new(py, vec![42, 21]).expect("this should not happen"))
                        .expect("comparison was not ok")
                );
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_with_unparseable_yes_vote_counts(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_YES as u16, "asdf", 1)
        .with_get_configstring(CS_VOTE_NO as u16, "21", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn current_vote_count_with_unparseable_no_vote_counts(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
        .with_get_configstring(CS_VOTE_NO as u16, "asdf", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn callvote_when_vote_is_active(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_STRING as u16, "map overkill ca", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::callvote(
                    &py.get_type::<Plugin>(),
                    "map thunderstruck ca",
                    "map thunderstruck ca",
                    30,
                );
                assert!(result.is_ok_and(|value| !value));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn callvote_calls_vote(_pyshinqlx_setup: ()) {
    let current_level_ctx = MockCurrentLevel::try_get_context();
    current_level_ctx.expect().returning(|| {
        let mut mock_level = MockCurrentLevel::new();
        mock_level
            .expect_callvote()
            .withf(|vote_str, vote_disp_str, opt_time| {
                vote_str == "map thunderstruck ca"
                    && vote_disp_str == "map thunderstruck ca"
                    && opt_time.is_some_and(|value| value == 30)
            })
            .times(1);
        Ok(mock_level)
    });

    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
        .run(|| {
            Python::attach(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                    .expect("could not add vote_started dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let result = Plugin::callvote(
                    &py.get_type::<Plugin>(),
                    "map thunderstruck ca",
                    "map thunderstruck ca",
                    30,
                );
                assert!(result.is_ok_and(|value| value),);
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn callvote_when_event_dispatcher_not_available(_pyshinqlx_setup: ()) {
    EVENT_DISPATCHERS.store(None);

    MockEngineBuilder::default()
        .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::callvote(
                    &py.get_type::<Plugin>(),
                    "map thunderstruck ca",
                    "map thunderstruck ca",
                    30,
                );
                assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn force_vote_with_unparseable_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::force_vote(
            &py.get_type::<Plugin>(),
            PyString::intern(py, "asdf").as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn force_vote_forces_vote_passed(_pyshinqlx_setup: ()) {
    let current_level_try_get_ctx = MockCurrentLevel::try_get_context();
    current_level_try_get_ctx.expect().returning(|| {
        let mut mock_level = MockCurrentLevel::new();
        mock_level.expect_get_vote_time().return_const(21);
        Ok(mock_level)
    });

    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(0))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_vote_state()
                .with(predicate::eq(true))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::eq(0), || {
            MockEngineBuilder::default().with_max_clients(1).run(|| {
                Python::attach(|py| {
                    let result = Plugin::force_vote(
                        &py.get_type::<Plugin>(),
                        PyBool::new(py, true).as_any(),
                    );
                    assert!(result.is_ok_and(|value| value),);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn force_vote_forces_vote_fail(_pyshinqlx_setup: ()) {
    let current_level_try_get_ctx = MockCurrentLevel::try_get_context();
    current_level_try_get_ctx.expect().returning(|| {
        let mut mock_level = MockCurrentLevel::new();
        mock_level.expect_get_vote_time().return_const(21);
        Ok(mock_level)
    });

    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(0))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_vote_state()
                .with(predicate::eq(false))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::eq(0), || {
            MockEngineBuilder::default().with_max_clients(1).run(|| {
                Python::attach(|py| {
                    let result = Plugin::force_vote(
                        &py.get_type::<Plugin>(),
                        PyBool::new(py, false).as_any(),
                    );
                    assert!(result.is_ok_and(|value| value),);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn teamsize_sets_teamsize(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_find_cvar(|cmd| cmd == "teamsize", |_| None, 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_get_cvar()
                .withf(|cvar, value, flags| cvar == "teamsize" && value == "42" && flags.is_none())
                .times(1);
        })
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::teamsize(&py.get_type::<Plugin>(), 42);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn kick_for_unknown_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::kick(
            &py.get_type::<Plugin>(),
            PyFloat::new(py, 1.23f64).as_any(),
            "",
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn kick_for_existing_player_without_reason(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let drop_client_ctx = shinqlx_drop_client_context();
    drop_client_ctx
        .expect()
        .withf(|_client, reason| reason == "was kicked.")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::kick(
                &py.get_type::<Plugin>(),
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                "",
            );
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn kick_for_existing_player_with_reason(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let drop_client_ctx = shinqlx_drop_client_context();
    drop_client_ctx
        .expect()
        .withf(|_client, reason| reason == "All your base are belong to us!")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::kick(
                &py.get_type::<Plugin>(),
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                "All your base are belong to us!",
            );
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn shuffle_forces_shuffle(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("forceshuffle", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::shuffle(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn change_map_with_no_factory(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("map thunderstruck", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", None);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn change_map_with_factory(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("map thunderstruck ffa", 1)
        .run(|| {
            Python::attach(|py| {
                let result =
                    Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", Some("ffa"));
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn change_map_when_no_main_engine_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", Some("ffa"));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_with_invalid_player1(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::switch(
            &py.get_type::<Plugin>(),
            PyFloat::new(py, 1.23f64).as_any(),
            Bound::new(py, default_test_player())
                .expect("this should not happen")
                .as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_with_invalid_player2(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::switch(
            &py.get_type::<Plugin>(),
            Bound::new(py, default_test_player())
                .expect("this should not happen")
                .as_any(),
            PyFloat::new(py, 1.23f64).as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_with_players_on_same_team(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 0,
        player_info: PlayerInfo {
            client_id: 0,
            team: team_t::TEAM_RED as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 1,
        player_info: PlayerInfo {
            client_id: 1,
            team: team_t::TEAM_RED as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    Python::attach(|py| {
        let result = Plugin::switch(
            &py.get_type::<Plugin>(),
            Bound::new(py, player1)
                .expect("this should not happen")
                .as_any(),
            Bound::new(py, player2)
                .expect("this should not happen")
                .as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_with_players_on_different_team(_pyshinqlx_setup: ()) {
    let player1 = Player {
        id: 0,
        player_info: PlayerInfo {
            client_id: 0,
            team: team_t::TEAM_RED as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };
    let player2 = Player {
        id: 1,
        player_info: PlayerInfo {
            client_id: 1,
            team: team_t::TEAM_BLUE as i32,
            ..default_test_player_info()
        }
        .into(),
        ..default_test_player()
    };

    MockEngineBuilder::default()
        .with_execute_console_command("put 0 blue", 1)
        .with_execute_console_command("put 1 red", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::switch(
                    &py.get_type::<Plugin>(),
                    Bound::new(py, player1)
                        .expect("this should not happen")
                        .as_any(),
                    Bound::new(py, player2)
                        .expect("this should not happen")
                        .as_any(),
                );
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_sound_to_all_players(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "playSound sound/vo/midair.ogg")
        .times(1);

    Python::attach(|py| {
        let result = Plugin::play_sound(&py.get_type::<Plugin>(), "sound/vo/midair.ogg", None);
        assert!(result.is_ok_and(|value| value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_sound_to_a_specific_player(_pyshinqlx_setup: ()) {
    let player = default_test_player();

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "playSound sound/vo/midair.ogg")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::play_sound(
                &py.get_type::<Plugin>(),
                "sound/vo/midair.ogg",
                Some(player),
            );
            assert!(result.is_ok_and(|value| value),);
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_sound_with_empty_soundpath(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::play_sound(&py.get_type::<Plugin>(), "", None);
        assert!(result.is_ok_and(|value| !value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_sound_for_sound_containing_music(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::play_sound(&py.get_type::<Plugin>(), "music/sonic1.ogg", None);
        assert!(result.is_ok_and(|value| !value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_music_to_all_players(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "playMusic music/sonic1.ogg")
        .times(1);

    Python::attach(|py| {
        let result = Plugin::play_music(&py.get_type::<Plugin>(), "music/sonic1.ogg", None);
        assert!(result.is_ok_and(|value| value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_music_to_a_specific_player(_pyshinqlx_setup: ()) {
    let player = default_test_player();

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "playMusic music/sonic1.ogg")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result =
                Plugin::play_music(&py.get_type::<Plugin>(), "music/sonic1.ogg", Some(player));
            assert!(result.is_ok_and(|value| value),);
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_music_with_empty_musicpath(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::play_music(&py.get_type::<Plugin>(), "", None);
        assert!(result.is_ok_and(|value| !value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn play_music_for_music_containing_sound(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::play_music(&py.get_type::<Plugin>(), "sound/vo/midair.ogg", None);
        assert!(result.is_ok_and(|value| !value),);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn stop_sound_for_all_players(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "clearSounds")
        .times(1);

    Python::attach(|py| {
        let result = Plugin::stop_sound(&py.get_type::<Plugin>(), None);
        assert!(result.is_ok());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn stop_sound_for_a_specific_player(_pyshinqlx_setup: ()) {
    let player = default_test_player();

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "clearSounds")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::stop_sound(&py.get_type::<Plugin>(), Some(player));
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn stop_music_for_all_players(_pyshinqlx_setup: ()) {
    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_none() && cmd == "stopMusic")
        .times(1);

    Python::attach(|py| {
        let result = Plugin::stop_music(&py.get_type::<Plugin>(), None);
        assert!(result.is_ok());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn stop_music_for_a_specific_player(_pyshinqlx_setup: ()) {
    let player = default_test_player();

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(move |_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_ACTIVE);
            mock_client
        });

    let send_server_command_ctx = shinqlx_send_server_command_context();
    send_server_command_ctx
        .expect()
        .withf(|client, cmd| client.is_some() && cmd == "stopMusic")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::stop_music(&py.get_type::<Plugin>(), Some(player));
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn slap_for_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::slap(&py.get_type::<Plugin>(), py.None().bind(py), 42);
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn slap_for_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("slap 21 42", 1)
        .run(|| {
            Python::attach(|py| {
                let result =
                    Plugin::slap(&py.get_type::<Plugin>(), PyInt::new(py, 21i32).as_any(), 42);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn slay_for_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::slay(&py.get_type::<Plugin>(), py.None().bind(py));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn slay_for_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("slay 21", 1)
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::slay(&py.get_type::<Plugin>(), PyInt::new(py, 21i32).as_any());
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn timeout_pauses_game(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("timeout", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::timeout(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        })
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn timein_unpauses_game(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("timein", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::timein(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn allready_readies_all_players(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("allready", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::allready(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn pause_pauses_game(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("pause", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::pause(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unpause_unpauses_game(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("unpause", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::unpause(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn lock_with_invalid_team(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .configure(|_mock_engine| {})
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::lock(
                    &py.get_type::<Plugin>(),
                    Some(PyString::new(py, "invalid_team").as_any()),
                );
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn lock_with_no_team(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("lock", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::lock(&py.get_type::<Plugin>(), None));
            assert!(result.is_ok());
        });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[cfg_attr(miri, ignore)]
#[serial]
fn lock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("lock {}", locked_team.to_lowercase()), 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::lock(
                    &py.get_type::<Plugin>(),
                    Some(PyString::new(py, locked_team).as_any()),
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unlock_with_invalid_team(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .configure(|_mock_engine| {})
        .run(|| {
            Python::attach(|py| {
                let result = Plugin::unlock(
                    &py.get_type::<Plugin>(),
                    Some(PyString::new(py, "invalid_team").as_any()),
                );
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unlock_with_no_team(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("unlock", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::unlock(&py.get_type::<Plugin>(), None));
            assert!(result.is_ok());
        });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[cfg_attr(miri, ignore)]
#[serial]
fn unlock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("unlock {}", locked_team.to_lowercase()), 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::unlock(
                    &py.get_type::<Plugin>(),
                    Some(PyString::new(py, locked_team).as_any()),
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn put_with_invalid_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::put(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 2i32).as_any(),
            PyString::new(py, "invalid team").as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn put_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::put(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 2048i32).as_any(),
            PyString::new(py, "red").as_any(),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[cfg_attr(miri, ignore)]
#[serial]
fn put_put_player_on_a_specific_team(_pyshinqlx_setup: (), #[case] new_team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::put(
                    &py.get_type::<Plugin>(),
                    PyInt::new(py, 2i32).as_any(),
                    PyString::new(py, new_team).as_any(),
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn mute_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::mute(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn mute_mutes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("mute 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::mute(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unmute_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::unmute(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unmute_unmutes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("unmute 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::unmute(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn tempban_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::tempban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn tempban_tempbans_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("tempban 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::tempban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn ban_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::ban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn ban_bans_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("ban 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::ban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unban_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::unban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn unban_unbans_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("unban 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::unban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn opsay_sends_op_message(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("opsay asdf", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::opsay(&py.get_type::<Plugin>(), "asdf"));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addadmin_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::addadmin(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addadmin_adds_player_to_admins(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addadmin 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::addadmin(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addmod_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::addmod(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addmod_adds_player_to_moderators(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addmod 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::addmod(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn demote_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::demote(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn demote_demotes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("demote 2", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::demote(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn abort_aborts_game(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("map_restart", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::abort(&py.get_type::<Plugin>()));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addscore_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::addscore(
            &py.get_type::<Plugin>(),
            PyInt::new(py, 2048i32).as_any(),
            42,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addscore_adds_score_to_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addscore 2 42", 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::addscore(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any(), 42)
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn addteamscore_with_invalid_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::addteamscore(
            &py.get_type::<Plugin>(),
            PyString::new(py, "invalid_team").as_any(),
            42,
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[cfg_attr(miri, ignore)]
#[serial]
fn addteamscore_adds_score_to_team(_pyshinqlx_setup: (), #[case] team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("addteamscore {} 42", team.to_lowercase()), 1)
        .run(|| {
            let result = Python::attach(|py| {
                Plugin::addteamscore(
                    &py.get_type::<Plugin>(),
                    PyString::new(py, team).as_any(),
                    42,
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn setmatchtime_sets_match_time(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("setmatchtime 42", 1)
        .run(|| {
            let result = Python::attach(|py| Plugin::setmatchtime(&py.get_type::<Plugin>(), 42));
            assert!(result.is_ok());
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn logger_and_plugins_can_be_read_from_python(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let logger_name = plugin_instance
            .getattr("logger")
            .and_then(|logger| logger.getattr("name"))
            .map(|name| name.to_string());
        let plugins = plugin_instance.getattr("plugins");

        assert_eq!(
            logger_name.expect("could not get the logger"),
            "shinqlx.test_plugin"
        );
        assert!(plugins.is_ok_and(|plugins| plugins.is_instance_of::<PyDict>()));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_hook_can_be_called_from_python(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        EVENT_DISPATCHERS.store(None);
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");

        let result = plugin_instance.call_method1("remove_hook", ("team_switch", py.None()));

        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_can_be_called_from_python(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        COMMANDS.store(Some(
            Py::new(py, CommandInvoker::py_new())
                .expect("could not create command invoker in python")
                .into(),
        ));
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");
        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");

        let result = plugin_instance.call_method1("remove_command", ("slap", &command_handler));

        assert!(result.is_ok());
        assert!(
            COMMANDS
                .load()
                .as_ref()
                .expect("could not get access to commands")
                .getattr(py, intern!(py, "commands"))
                .expect("could not get commands")
                .cast_bound::<PyList>(py)
                .expect("could not downcast to list")
                .is_empty()
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn remove_command_without_command_invoker_fails(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let command_handler = python_function_returning(py, &py.None());
        COMMANDS.store(Some(
            Py::new(py, CommandInvoker::py_new())
                .expect("could not create command invoker in python")
                .into(),
        ));
        let plugin_instance = test_plugin(py)
            .call0()
            .expect("could not create plugin instance");
        plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .add_command(
                PyString::intern(py, "slap").as_any(),
                &command_handler,
                0,
                None,
                None,
                CommandPriorities::PRI_NORMAL as u32,
                false,
                0,
                true,
                "",
            )
            .expect("could not add command");
        COMMANDS.store(None);

        let result = plugin_instance
            .cast::<Plugin>()
            .expect("could not downcast instance to plugin")
            .remove_command(PyString::intern(py, "slap").as_any(), &command_handler);

        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn tell_with_an_unknown_recipient_fails(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let result = Plugin::tell(&py.get_type::<Plugin>(), "asdf", py.None().bind(py), None);

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    });
}
