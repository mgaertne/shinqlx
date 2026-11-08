mod chat_event_dispatcher;
mod client_command_dispatcher;
mod command_dispatcher;
mod console_print_dispatcher;
mod damage_dispatcher;
mod death_dispatcher;
mod frame_event_dispatcher;
mod game_countdown_dispatcher;
mod game_end_dispatcher;
mod game_start_dispatcher;
mod kamikaze_explode_dispatcher;
mod kamikaze_use_dispatcher;
mod kill_dispatcher;
mod map_dispatcher;
mod new_game_dispatcher;
mod player_connect_dispatcher;
mod player_disconnect_dispatcher;
mod player_loaded_dispatcher;
mod player_spawn_dispatcher;
mod round_countdown_dispatcher;
mod round_end_dispatcher;
mod round_start_dispatcher;
mod server_command_dispatcher;
mod set_configstring_dispatcher;
mod stats_dispatcher;
mod team_switch_attempt_dispatcher;
mod team_switch_dispatcher;
mod unload_dispatcher;
mod userinfo_dispatcher;
mod vote_called_dispatcher;
mod vote_dispatcher;
mod vote_ended_dispatcher;
mod vote_started_dispatcher;

mod prelude {
    pub(crate) use pyo3::{intern, prelude::*};

    pub(crate) use super::{
        super::{PythonReturnCodes, log_exception, pyshinqlx_get_logger},
        EventDispatcher, EventDispatcherMethods, dispatcher_debug_log, log_unexpected_return_value,
    };
}

use core::hint::cold_path;

pub(crate) use chat_event_dispatcher::{ChatEventDispatcher, ChatEventDispatcherMethods};
pub(crate) use client_command_dispatcher::{
    ClientCommandDispatcher, ClientCommandDispatcherMethods,
};
pub(crate) use command_dispatcher::{CommandDispatcher, CommandDispatcherMethods};
pub(crate) use console_print_dispatcher::{ConsolePrintDispatcher, ConsolePrintDispatcherMethods};
pub(crate) use damage_dispatcher::{DamageDispatcher, DamageDispatcherMethods};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use death_dispatcher::{DeathDispatcher, DeathDispatcherMethods};
pub(crate) use frame_event_dispatcher::{FrameEventDispatcher, FrameEventDispatcherMethods};
pub(crate) use game_countdown_dispatcher::{
    GameCountdownDispatcher, GameCountdownDispatcherMethods,
};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use game_end_dispatcher::{GameEndDispatcher, GameEndDispatcherMethods};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use game_start_dispatcher::{GameStartDispatcher, GameStartDispatcherMethods};
use itertools::Itertools;
pub(crate) use kamikaze_explode_dispatcher::{
    KamikazeExplodeDispatcher, KamikazeExplodeDispatcherMethods,
};
pub(crate) use kamikaze_use_dispatcher::{KamikazeUseDispatcher, KamikazeUseDispatcherMethods};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use kill_dispatcher::{KillDispatcher, KillDispatcherMethods};
pub(crate) use map_dispatcher::{MapDispatcher, MapDispatcherMethods};
pub(crate) use new_game_dispatcher::{NewGameDispatcher, NewGameDispatcherMethods};
pub(crate) use player_connect_dispatcher::{
    PlayerConnectDispatcher, PlayerConnectDispatcherMethods,
};
pub(crate) use player_disconnect_dispatcher::{
    PlayerDisconnectDispatcher, PlayerDisconnectDispatcherMethods,
};
pub(crate) use player_loaded_dispatcher::{PlayerLoadedDispatcher, PlayerLoadedDispatcherMethods};
pub(crate) use player_spawn_dispatcher::{PlayerSpawnDispatcher, PlayerSpawnDispatcherMethods};
use prelude::*;
use pyo3::{
    PyTraverseError, PyVisit,
    exceptions::{PyAssertionError, PyAttributeError, PyKeyError, PyValueError},
    types::{IntoPyDict, PyBool, PyDict, PyTuple, PyType},
};
pub(crate) use round_countdown_dispatcher::{
    RoundCountdownDispatcher, RoundCountdownDispatcherMethods,
};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use round_end_dispatcher::{RoundEndDispatcher, RoundEndDispatcherMethods};
pub(crate) use round_start_dispatcher::{RoundStartDispatcher, RoundStartDispatcherMethods};
pub(crate) use server_command_dispatcher::{
    ServerCommandDispatcher, ServerCommandDispatcherMethods,
};
pub(crate) use set_configstring_dispatcher::{
    SetConfigstringDispatcher, SetConfigstringDispatcherMethods,
};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use stats_dispatcher::{StatsDispatcher, StatsDispatcherMethods};
use tap::TapFallible;
pub(crate) use team_switch_attempt_dispatcher::{
    TeamSwitchAttemptDispatcher, TeamSwitchAttemptDispatcherMethods,
};
#[allow(
    unused_imports,
    reason = "re-exported for the prelude, not every build uses every item"
)]
pub(crate) use team_switch_dispatcher::{TeamSwitchDispatcher, TeamSwitchDispatcherMethods};
use unicode_segmentation::UnicodeSegmentation;
pub(crate) use unload_dispatcher::{UnloadDispatcher, UnloadDispatcherMethods};
pub(crate) use userinfo_dispatcher::{UserinfoDispatcher, UserinfoDispatcherMethods};
pub(crate) use vote_called_dispatcher::{VoteCalledDispatcher, VoteCalledDispatcherMethods};
pub(crate) use vote_dispatcher::{VoteDispatcher, VoteDispatcherMethods};
pub(crate) use vote_ended_dispatcher::{VoteEndedDispatcher, VoteEndedDispatcherMethods};
pub(crate) use vote_started_dispatcher::{VoteStartedDispatcher, VoteStartedDispatcherMethods};

use super::{commands::CommandPriorities, get_cvar, log_debug};

fn try_dispatcher_debug_log(py: Python<'_>, debug_str: &str) -> PyResult<()> {
    if debug_str.len() > 100 {
        let dbgstr = format!(
            "{})",
            debug_str.graphemes(true).take(99).collect::<String>()
        );
        log_debug(py, "dispatch", &dbgstr)
    } else {
        log_debug(py, "dispatch", debug_str)
    }
}

pub(crate) fn dispatcher_debug_log(py: Python<'_>, debug_str: &str) {
    let _ = try_dispatcher_debug_log(py, debug_str).tap_err(|e| {
        log_exception(py, e);
    });
}

fn try_log_unexpected_return_value(
    py: Python<'_>,
    event_name: &str,
    result: &Bound<'_, PyAny>,
    handler: &Bound<'_, PyAny>,
) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let warning_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "WARNING")))?;
        let handler_name = handler.getattr(intern!(py, "__name__"))?;

        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    warning_level,
                    intern!(py, ""),
                    -1,
                    intern!(
                        py,
                        "Handler '%s' returned unknown value '%s' for event '%s'"
                    ),
                    (handler_name, result, event_name),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    Ok(())
}

pub(crate) fn log_unexpected_return_value(
    py: Python<'_>,
    event_name: &str,
    result: &Bound<'_, PyAny>,
    handler: &Bound<'_, PyAny>,
) {
    let _ = try_log_unexpected_return_value(py, event_name, result, handler).tap_err(|e| {
        log_exception(py, e);
    });
}

#[pyclass(name = "EventDispatcher", module = "_events", subclass, frozen)]
pub(crate) struct EventDispatcher {
    #[allow(
        clippy::type_complexity,
        reason = "keeps the handlers per priority next to the plugin name"
    )]
    plugins: parking_lot::RwLock<Vec<(String, [Vec<Py<PyAny>>; 5])>>,
}

const NO_DEBUG: [&str; 9] = [
    "frame",
    "set_configstring",
    "stats",
    "server_command",
    "death",
    "kill",
    "command",
    "console_print",
    "damage",
];

#[cfg(feature = "bench-support")]
impl EventDispatcher {
    /// Hooks a handler for the benchmarks, which run without an engine to read the cvars from.
    pub(super) fn hook_without_checks(&self, plugin: &str, handler: Py<PyAny>) {
        self.plugins.write().push((
            plugin.to_string(),
            [vec![], vec![], vec![handler], vec![], vec![]],
        ));
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self {
            plugins: parking_lot::RwLock::new(Vec::new()),
        }
    }
}
#[pymethods]
impl EventDispatcher {
    #[classattr]
    fn no_debug() -> (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    ) {
        NO_DEBUG.iter().cloned().collect_tuple().unwrap_or_default()
    }

    #[new]
    pub(crate) fn py_new(_py: Python<'_>) -> Self {
        Self::default()
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.plugins
            .read()
            .iter()
            .flat_map(|(_, plugins)| {
                plugins
                    .iter()
                    .flat_map(|prio_plugins| prio_plugins.iter().map(|plugin| visit.call(plugin)))
            })
            .collect::<Result<Vec<_>, PyTraverseError>>()
            .map(|_| ())
    }

    fn __clear__(&self) {
        self.plugins.write().iter_mut().for_each(|(_, plugins)| {
            plugins.iter_mut().for_each(|prio_plugins| {
                prio_plugins.clear();
            })
        });
    }

    #[getter(plugins)]
    fn get_plugins<'py>(slf: &Bound<'py, Self>) -> Bound<'py, PyDict> {
        slf.get_plugins()
    }

    /// Calls all the handlers that have been registered when hooking this event.
    /// The recommended way to use this for events that inherit this class is to
    /// override the method with explicit arguments (as opposed to this one's)
    /// and call this method by using ``super().dispatch()``.
    ///
    /// Handlers have several options for return values that can affect the flow:
    ///     - shinqlx.RET_NONE or None -- Continue execution normally.
    ///     - shinqlx.RET_STOP -- Stop any further handlers from being called.
    ///     - shinqlx.RET_STOP_EVENT -- Let handlers process it, but stop the event
    ///         at the engine-level.
    ///     - shinqlx.RET_STOP_ALL -- Stop handlers **and** the event.
    ///     - Any other value -- Passed on to :func:`self.handle_return`, which will
    ///         by default simply send a warning to the logger about an unknown value
    ///         being returned. Can be overridden so that events can have their own
    ///         special return values.
    #[pyo3(signature = (*args))]
    fn dispatch<'py>(slf: &Bound<'py, Self>, args: &Bound<'py, PyTuple>) -> Bound<'py, PyAny> {
        slf.dispatch(args)
    }

    /// Handle an unknown return value. If this returns anything but None,
    /// it will stop execution of the event and pass the return value on
    /// to the C-level handlers. This method can be useful to override,
    /// because of the fact that you can edit the arguments that will be
    /// passed on to any handler after whatever handler returned *value*
    /// by editing *self.args*. Furthermore, *self.return_value*
    /// is the return value that will be sent to the C-level handler if the
    /// event isn't stopped later along the road.
    fn handle_return<'py>(
        slf: &Bound<'py, Self>,
        handler: &Bound<'py, PyAny>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.handle_return(handler, value)
    }

    /// Hook the event, making the handler get called with relevant arguments
    /// whenever the event is takes place.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "($self, plugin, handler, priority=PRI_NORMAL)")]
    fn add_hook(
        slf: &Bound<'_, Self>,
        plugin: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.add_hook(plugin, handler, priority)
    }

    /// Removes a previously hooked event.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "($self, plugin, handler, priority=PRI_NORMAL)")]
    fn remove_hook(
        slf: &Bound<'_, Self>,
        plugin: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.remove_hook(plugin, handler, priority)
    }
}

pub(crate) trait EventDispatcherMethods<'py> {
    fn get_plugins(&self) -> Bound<'py, PyDict>;
    /// Whether at least one handler is hooked to this dispatcher. Events that are not logged
    /// can skip building their arguments if nobody is listening.
    fn has_hooks(&self) -> bool;
    fn dispatch(&self, args: &Bound<'py, PyTuple>) -> Bound<'py, PyAny>;
    fn handle_return(
        &self,
        handler: &Bound<'py, PyAny>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn add_hook(&self, plugin: &str, handler: &Bound<'py, PyAny>, priority: i32) -> PyResult<()>;
    fn remove_hook(&self, plugin: &str, handler: &Bound<'py, PyAny>, priority: i32)
    -> PyResult<()>;
}

impl<'py> EventDispatcherMethods<'py> for Bound<'py, EventDispatcher> {
    fn get_plugins(&self) -> Bound<'py, PyDict> {
        self.get()
            .plugins
            .try_read()
            .and_then(|plugins| {
                plugins
                    .iter()
                    .map(|(plugin_name, hooks)| {
                        (
                            plugin_name.to_owned(),
                            hooks
                                .iter()
                                .map(|prio_hooks| {
                                    prio_hooks
                                        .iter()
                                        .map(|hook| hook.clone_ref(self.py()))
                                        .collect()
                                })
                                .collect(),
                        )
                    })
                    .collect::<Vec<(String, Vec<Vec<Py<PyAny>>>)>>()
                    .into_py_dict(self.py())
                    .ok()
            })
            .unwrap_or_else(|| PyDict::new(self.py()))
    }

    fn has_hooks(&self) -> bool {
        self.get().plugins.read().iter().any(|(_, hooks)| {
            hooks
                .iter()
                .any(|priority_hooks| !priority_hooks.is_empty())
        })
    }

    fn dispatch(&self, args: &Bound<'py, PyTuple>) -> Bound<'py, PyAny> {
        let Ok(py_dispatcher_name) = self.get_type().getattr(intern!(self.py(), "name")) else {
            cold_path();
            return self.py().None().into_bound(self.py());
        };
        let Ok(dispatcher_name) = py_dispatcher_name.extract::<&str>() else {
            cold_path();
            return self.py().None().into_bound(self.py());
        };
        if !NO_DEBUG.contains(&dispatcher_name) {
            let dbgstr = format!("{dispatcher_name}{args}");
            dispatcher_debug_log(self.py(), &dbgstr);
        }

        let mut return_value = PyBool::new(self.py(), true).to_owned().into_any().unbind();

        let plugins = self.get().plugins.read();
        for handler in (0..5).flat_map(|i| {
            plugins.iter().flat_map(move |(_, handlers)| {
                handlers[i]
                    .iter()
                    .map(|handler| handler.clone_ref(self.py()))
            })
        }) {
            match handler.call1(self.py(), args) {
                Err(e) => {
                    cold_path();
                    log_exception(self.py(), &e);
                }
                Ok(res) => match res.extract::<PythonReturnCodes>(self.py()) {
                    Ok(PythonReturnCodes::RET_NONE) => (),
                    Ok(PythonReturnCodes::RET_STOP) => {
                        return PyBool::new(self.py(), true).to_owned().into_any();
                    }
                    Ok(PythonReturnCodes::RET_STOP_EVENT) => {
                        return_value = PyBool::new(self.py(), false).to_owned().into_any().unbind();
                    }
                    Ok(PythonReturnCodes::RET_STOP_ALL) => {
                        return PyBool::new(self.py(), false).to_owned().into_any();
                    }
                    _ => {
                        match self.call_method1(
                            intern!(self.py(), "handle_return"),
                            (handler.bind(self.py()), res),
                        ) {
                            Err(e) => {
                                cold_path();
                                log_exception(self.py(), &e);
                            }
                            Ok(return_handler) if !return_handler.is_none() => {
                                return return_handler;
                            }
                            _ => (),
                        }
                    }
                },
            }
        }

        return_value.bind(self.py()).to_owned()
    }

    fn handle_return(
        &self,
        handler: &Bound<'py, PyAny>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let dispatcher_name = self
            .get_type()
            .getattr(intern!(self.py(), "name"))?
            .extract::<String>()?;
        log_unexpected_return_value(self.py(), &dispatcher_name, value, handler);

        Ok(self.py().None().into_bound(self.py()))
    }

    fn add_hook(&self, plugin: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        if !(0i32..5i32).contains(&priority) {
            cold_path();
            let error_description = format!("'{priority}' is an invalid priority level.");
            return Err(PyValueError::new_err(error_description));
        }

        let dispatcher_name = self
            .get_type()
            .getattr(intern!(self.py(), "name"))
            .and_then(|name| name.extract::<String>())
            .map_err(|_| {
                PyAttributeError::new_err(
                    "Cannot add a hook from an event dispatcher with no name.",
                )
            })?;

        let need_zmq_stats_enabled = self
            .get_type()
            .getattr(intern!(self.py(), "need_zmq_stats_enabled"))
            .and_then(|needs_zmq| needs_zmq.extract::<bool>())
            .map_err(|_| {
                PyAttributeError::new_err("Cannot add a hook from an event dispatcher with no need_zmq_stats_enabled flag.")
            })?;

        let zmq_enabled_cvar = get_cvar("zmq_stats_enable")?;
        let zmq_enabled = zmq_enabled_cvar.is_some_and(|value| value != "0");
        if need_zmq_stats_enabled && !zmq_enabled {
            cold_path();
            let error_description = format!(
                "{dispatcher_name} hook requires zmq_stats_enabled cvar to have nonzero value"
            );
            return Err(PyAssertionError::new_err(error_description));
        }

        match self.get().plugins.try_write() {
            None => {
                cold_path();
                let add_hook_func = PyModule::from_code(
                    self.py(),
                    cr#"
import shinqlx


@shinqlx.next_frame
def add_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].add_hook(plugin, handler, priority)
        "#,
                    c"",
                    c"",
                )?
                .getattr(intern!(self.py(), "add_hook"))?;

                add_hook_func.call1((&dispatcher_name, plugin, handler, priority))?;
            }
            Some(mut plugins) => {
                let Some(plugin_hooks) = plugins
                    .iter_mut()
                    .find(|(added_plugin, _)| added_plugin == plugin)
                else {
                    let mut new_hooks =
                        (plugin.to_string(), [vec![], vec![], vec![], vec![], vec![]]);
                    new_hooks.1[priority as usize].push(handler.to_owned().unbind());
                    plugins.push(new_hooks);
                    return Ok(());
                };

                if plugin_hooks.1[priority as usize]
                    .iter()
                    .any(|registered_command| {
                        registered_command
                            .bind(self.py())
                            .eq(handler)
                            .unwrap_or(false)
                    })
                {
                    cold_path();
                    return Err(PyValueError::new_err(
                        "The event has already been hooked with the same handler and priority.",
                    ));
                }

                plugin_hooks.1[priority as usize].push(handler.to_owned().unbind());
            }
        }
        Ok(())
    }

    fn remove_hook(&self, plugin: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        let dispatcher_name = self
            .get_type()
            .getattr(intern!(self.py(), "name"))
            .and_then(|value| value.extract::<String>())
            .map_err(|_| {
                PyAttributeError::new_err(
                    "Cannot remove a hook from an event dispatcher with no name.",
                )
            })?;
        match self.get().plugins.try_write() {
            None => {
                cold_path();
                let remove_hook_func = PyModule::from_code(
                    self.py(),
                    cr#"
import shinqlx


@shinqlx.next_frame
def remove_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].remove_hook(plugin, handler, priority)
        "#,
                    c"",
                    c"",
                )?
                .getattr(intern!(self.py(), "remove_hook"))?;
                remove_hook_func.call1((dispatcher_name, plugin, handler, priority))?;
            }
            Some(mut plugins) => {
                let Some(plugin_hooks) = plugins
                    .iter_mut()
                    .find(|(added_plugin, _)| added_plugin == plugin)
                else {
                    cold_path();
                    return Err(PyValueError::new_err(
                        "The event has not been hooked with the handler provided",
                    ));
                };

                if !plugin_hooks.1[priority as usize]
                    .iter()
                    .any(|item| item.bind(self.py()).eq(handler).unwrap_or(true))
                {
                    cold_path();
                    return Err(PyValueError::new_err(
                        "The event has not been hooked with the handler provided",
                    ));
                }

                plugin_hooks.1[priority as usize]
                    .retain(|item| item.bind(self.py()).ne(handler).unwrap_or(true));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod event_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        exceptions::{PyAssertionError, PyAttributeError, PyValueError},
        intern,
        prelude::*,
        types::{PyBool, PyDict, PyTuple},
    };
    use rstest::*;

    use super::{EventDispatcher, EventDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                PythonReturnCodes,
                commands::CommandPriorities,
                pyshinqlx_setup_fixture::*,
                pyshinqlx_test_support::{
                    python_function_raising_exception, python_function_returning,
                },
            },
        },
        prelude::*,
    };

    fn custom_dispatcher(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            cr#"
import shinqlx

class CustomDispatcher(shinqlx.EventDispatcher):
    name = "custom_event"
    need_zmq_stats_enabled = False

    def __init__(self):
        super().__init__()
        "#,
            c"",
            c"",
        )
        .expect("this should not happen")
        .getattr(intern!(py, "CustomDispatcher"))
        .expect("this should not happen")
        .call0()
        .expect("this should not happen")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn has_hooks_when_none_are_registered(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher = custom_dispatcher(py);

            assert!(
                !dispatcher
                    .cast::<EventDispatcher>()
                    .expect("this should not happen")
                    .has_hooks()
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn has_hooks_with_a_hook_registered(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_LOWEST as i32,
                            ),
                        )
                        .expect("this should not happen");

                    assert!(
                        dispatcher
                            .cast::<EventDispatcher>()
                            .expect("this should not happen")
                            .has_hooks()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_plugins_when_none_are_registered(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher = custom_dispatcher(py);

            let result = dispatcher.getattr(intern!(py, "plugins"));
            assert!(
                result.is_ok_and(|value| value.cast::<PyDict>().is_ok_and(|dict| dict.is_empty()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_plugins_with_plugins_registered(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "prio5_plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_LOWEST as i32,
                            ),
                        )
                        .expect("this should not happen");
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "prio4_plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_LOW as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "prio3_plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "prio2_plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_HIGH as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "prio1_plugin",
                                python_function_returning(py, &py.None().into_bound(py)),
                                CommandPriorities::PRI_HIGHEST as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result = dispatcher.getattr(intern!(py, "plugins"));
                    assert!(result.is_ok_and(|value| {
                        value.cast::<PyDict>().is_ok_and(|dict| dict.len() == 5)
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher = custom_dispatcher(py);

            let result = dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
            assert!(result.is_ok_and(|value| {
                value
                    .cast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_exception(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let throws_exception_hook = python_function_raising_exception(py);
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                throws_exception_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_none(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_none_hook =
                        python_function_returning(py, &py.None().into_bound(py));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_none_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_none(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_none_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_NONE as i32));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_none_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP as i32));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_event(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_event_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP_EVENT as i32));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_event_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_all(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_all_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP_ALL as i32));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_all_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let returns_string_hook = python_function_returning(py, &"return string");
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_string_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string_and_return_handler_throws_exception(
        _pyshinqlx_setup: (),
    ) {
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
                    let dispatcher = custom_dispatcher(py);
                    let return_handler = python_function_raising_exception(py);
                    dispatcher
                        .setattr(intern!(py, "handle_return"), return_handler)
                        .expect("this should not happen");

                    let returns_string_hook = python_function_returning(py, &"return string");
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_string_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .cast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string_and_return_handler_returns_string(
        _pyshinqlx_setup: (),
    ) {
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
                    let dispatcher = custom_dispatcher(py);
                    let return_handler =
                        python_function_returning(py, &"return_handler string return");
                    dispatcher
                        .setattr(intern!(py, "handle_return"), return_handler)
                        .expect("this should not happen");

                    let returns_string_hook = python_function_returning(py, &"return string");
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_string_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "return_handler string return")
                    }));
                });
            });
    }

    #[rstest]
    #[case(-1i32)]
    #[case(6i32)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_with_wrong_priority(#[case] priority: i32, _pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher = custom_dispatcher(py);

            let default_hook = python_function_returning(py, &py.None().into_bound(py));
            let result = dispatcher.call_method1(
                intern!(py, "add_hook"),
                ("test_plugin", default_hook.unbind(), priority),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_for_dispatcher_with_no_name(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let nameless_dispatcher = PyModule::from_code(
                py,
                cr#"
import shinqlx

class NamelessDispatcher(shinqlx.EventDispatcher):
    need_zmq_stats_enabled = False

    def __init__(self):
        super().__init__()
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr(intern!(py, "NamelessDispatcher"))
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = python_function_returning(py, &py.None().into_bound(py));
            let result = nameless_dispatcher.call_method1(
                intern!(py, "add_hook"),
                (
                    "test_plugin",
                    default_hook.unbind(),
                    CommandPriorities::PRI_NORMAL as i32,
                ),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_for_dispatcher_with_no_zmq_enabled_flag(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let zmq_less_dispatcher = PyModule::from_code(
                py,
                cr#"
import shinqlx

class ZmqLessDispatcher(shinqlx.EventDispatcher):
    name = "ZmqLessDispatcher"

    def __init__(self):
        super().__init__()
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr(intern!(py, "ZmqLessDispatcher"))
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = python_function_returning(py, &py.None().into_bound(py));
            let result = zmq_less_dispatcher.call_method1(
                intern!(py, "add_hook"),
                (
                    "test_plugin",
                    default_hook.unbind(),
                    CommandPriorities::PRI_NORMAL as i32,
                ),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_for_zmq_enabled_dispatcher_when_zmq_disabled(_pyshinqlx_setup: ()) {
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
                    let zmq_dispatcher = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

class ZmqEnabledDispatcher(shinqlx.EventDispatcher):
    name = "zmq_event"
    need_zmq_stats_enabled = True

    def __init__(self):
        super().__init__()
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr(intern!(py, "ZmqEnabledDispatcher"))
                    .expect("this should not happen")
                    .call0()
                    .expect("this should not happen");

                    let default_hook = python_function_returning(py, &py.None().into_bound(py));
                    let result = zmq_dispatcher.call_method1(
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            default_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    );

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_when_handler_already_was_added(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let default_hook = python_function_returning(py, &py.None().into_bound(py));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                default_hook.to_owned().unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result = dispatcher.call_method1(
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            default_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    );

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_for_dispatcher_with_no_name(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let nameless_dispatcher = PyModule::from_code(
                py,
                cr#"
import shinqlx

class NamelessDispatcher(shinqlx.EventDispatcher):
    need_zmq_stats_enabled = False

    def __init__(self):
        super().__init__()
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr(intern!(py, "NamelessDispatcher"))
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = python_function_returning(py, &py.None().into_bound(py));
            let result = nameless_dispatcher.call_method1(
                intern!(py, "remove_hook"),
                (
                    "test_plugin",
                    default_hook.unbind(),
                    CommandPriorities::PRI_NORMAL as i32,
                ),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_for_handler_that_was_not_added(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher = custom_dispatcher(py);

            let default_hook = python_function_returning(py, &py.None().into_bound(py));
            let result = dispatcher.call_method1(
                intern!(py, "remove_hook"),
                (
                    "test_plugin",
                    default_hook.unbind(),
                    CommandPriorities::PRI_NORMAL as i32,
                ),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_for_handler_that_was_added_with_different_priority(_pyshinqlx_setup: ()) {
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
                    let dispatcher = custom_dispatcher(py);

                    let default_hook = python_function_returning(py, &py.None().into_bound(py));
                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                default_hook.to_owned().unbind(),
                                CommandPriorities::PRI_LOWEST as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result = dispatcher.call_method1(
                        intern!(py, "remove_hook"),
                        (
                            "test_plugin",
                            default_hook.unbind(),
                            CommandPriorities::PRI_HIGHEST as i32,
                        ),
                    );

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }
}

/// Holds all the event dispatchers and provides a way to access the dispatcher
/// instances by accessing it like a dictionary using the event name as a key.
/// Only one dispatcher can be used per event.
#[pyclass(name = "EventDispatcherManager", module = "_events", mapping, frozen)]
#[derive(Default)]
pub(crate) struct EventDispatcherManager {
    dispatchers: parking_lot::RwLock<Vec<(String, Py<PyAny>)>>,
}

#[pymethods]
impl EventDispatcherManager {
    #[new]
    fn py_new(py: Python<'_>) -> Self {
        py.detach(Self::default)
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.dispatchers
            .read()
            .iter()
            .map(|(_, plugins)| visit.call(plugins))
            .collect::<Result<Vec<_>, PyTraverseError>>()
            .map(|_| ())
    }

    fn __clear__(&self) {
        self.dispatchers.write().clear();
    }

    #[getter(_dispatchers)]
    fn get_dispatchers<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyDict>> {
        slf.get_dispatchers()
    }

    pub(crate) fn __getitem__<'py>(
        slf: &Bound<'py, Self>,
        key: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.__getitem__(key)
    }

    fn __contains__(slf: &Bound<'_, Self>, key: &str) -> bool {
        slf.__contains__(key)
    }

    pub(crate) fn add_dispatcher(
        slf: &Bound<'_, Self>,
        dispatcher: &Bound<'_, PyType>,
    ) -> PyResult<()> {
        slf.add_dispatcher(dispatcher)
    }

    fn remove_dispatcher(slf: &Bound<'_, Self>, dispatcher: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.remove_dispatcher(dispatcher)
    }

    fn remove_dispatcher_by_name(slf: &Bound<'_, Self>, event_name: &str) -> PyResult<()> {
        slf.remove_dispatcher_by_name(event_name)
    }
}

pub(crate) trait EventDispatcherManagerMethods<'py> {
    fn __contains__(&self, key: &str) -> bool;
    fn __getitem__(&self, key: &str) -> PyResult<Bound<'py, PyAny>>;
    fn get_dispatchers(&self) -> PyResult<Bound<'py, PyDict>>;
    fn add_dispatcher(&self, dispatcher: &Bound<'py, PyType>) -> PyResult<()>;
    fn remove_dispatcher(&self, dispatcher: &Bound<'py, PyAny>) -> PyResult<()>;
    fn remove_dispatcher_by_name(&self, dispatcher_name: &str) -> PyResult<()>;
}

impl<'py> EventDispatcherManagerMethods<'py> for Bound<'py, EventDispatcherManager> {
    fn __contains__(&self, key: &str) -> bool {
        self.get()
            .dispatchers
            .read()
            .iter()
            .any(|(event_name, _)| key == event_name)
    }

    fn __getitem__(&self, key: &str) -> PyResult<Bound<'py, PyAny>> {
        self.get()
            .dispatchers
            .read()
            .iter()
            .find(|(event_name, _)| key == event_name)
            .map_or_else(
                || {
                    cold_path();
                    let key_error = format!("'{key}'");
                    Err(PyKeyError::new_err(key_error))
                },
                |(_, dispatcher)| Ok(dispatcher.bind(self.py()).to_owned()),
            )
    }

    fn get_dispatchers(&self) -> PyResult<Bound<'py, PyDict>> {
        self.get()
            .dispatchers
            .read()
            .iter()
            .map(|(dispatcher_name, dispatch_function)| {
                (
                    dispatcher_name.to_owned(),
                    dispatch_function.bind(self.py()).as_unbound(),
                )
            })
            .collect::<Vec<(String, &Py<PyAny>)>>()
            .into_py_dict(self.py())
    }

    fn add_dispatcher(&self, dispatcher: &Bound<'py, PyType>) -> PyResult<()> {
        if !dispatcher
            .is_subclass_of::<EventDispatcher>()
            .unwrap_or(false)
        {
            cold_path();
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher not based on EventDispatcher.",
            ));
        }

        let dispatcher_name_str = dispatcher
            .getattr(intern!(self.py(), "name"))
            .and_then(|dispatcher_name_attr| dispatcher_name_attr.extract::<String>())
            .map_err(|_| PyValueError::new_err("Cannot add an event dispatcher with no name."))?;
        if self.__contains__(&dispatcher_name_str) {
            cold_path();
            return Err(PyValueError::new_err("Event name already taken."));
        }

        self.get()
            .dispatchers
            .write()
            .push((dispatcher_name_str, dispatcher.call0()?.unbind()));

        Ok(())
    }

    fn remove_dispatcher(&self, dispatcher: &Bound<'py, PyAny>) -> PyResult<()> {
        let dispatcher_name_str = dispatcher
            .getattr(intern!(self.py(), "name"))
            .and_then(|dispatcher_name_attr| dispatcher_name_attr.extract::<String>())
            .map_err(|_| {
                PyValueError::new_err("Cannot remove an event dispatcher with no name.")
            })?;

        self.remove_dispatcher_by_name(&dispatcher_name_str)
    }

    fn remove_dispatcher_by_name(&self, dispatcher_name: &str) -> PyResult<()> {
        if !self.__contains__(dispatcher_name) {
            cold_path();
            return Err(PyValueError::new_err("Event name not found."));
        }

        match self.get().dispatchers.try_write() {
            None => {
                cold_path();
                let remove_dispatcher_by_name_func = PyModule::from_code(
                    self.py(),
                    cr#"
import shinqlx


@shinqlx.next_frame
def remove_dispatcher_by_name(dispatcher_name):
    shinqlx.EVENT_DISPATCHERS.remove_dispatcher_by_name(dispatcher_name)
        "#,
                    c"",
                    c"",
                )?
                .getattr(intern!(self.py(), "remove_dispatcher_by_name"))?;

                remove_dispatcher_by_name_func.call1((dispatcher_name,))?;
            }
            Some(mut dispatchers) => {
                dispatchers.retain(|(name, _)| name != dispatcher_name);
            }
        };

        Ok(())
    }
}

#[cfg(test)]
mod event_dispatcher_manager_tests {
    use pyo3::{
        exceptions::{PyKeyError, PyValueError},
        intern,
        prelude::*,
    };
    use rstest::*;

    use super::{
        EventDispatcherManager, EventDispatcherManagerMethods, GameCountdownDispatcher,
        GameEndDispatcher, GameStartDispatcher,
    };
    use crate::ffi::python::{plugin::Plugin, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn event_dispatcher_manager_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            let result = py
                .import(intern!(py, "gc"))
                .and_then(|gc| gc.call_method0(intern!(py, "collect")));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_when_no_dispatchers_added(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.get_dispatchers();
            assert!(result.is_ok_and(|dict| dict.is_empty()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            let result = event_dispatchers
                .get_dispatchers()
                .expect("this should not happen");
            assert!(
                result
                    .get_item(intern!(py, "game_countdown"))
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameCountdownDispatcher>()
                    }))
            );
            assert!(
                result
                    .get_item(intern!(py, "game_start"))
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameStartDispatcher>()
                    }))
            );
            assert!(
                result
                    .get_item(intern!(py, "game_end"))
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameEndDispatcher>()
                    }))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_item_from_empty_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.get_item(intern!(py, "game_start"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_item_for_existing_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");

            let result = event_dispatchers.get_item(intern!(py, "game_start"));
            assert!(
                result.is_ok_and(|dispatcher| dispatcher.is_instance_of::<GameStartDispatcher>())
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            assert!(
                event_dispatchers
                    .contains(intern!(py, "game_countdown"))
                    .is_ok_and(|contained| contained)
            );
            assert!(
                event_dispatchers
                    .contains(intern!(py, "game_start"))
                    .is_ok_and(|contained| contained)
            );
            assert!(
                event_dispatchers
                    .contains(intern!(py, "game_end"))
                    .is_ok_and(|contained| contained)
            );
            assert!(
                !event_dispatchers
                    .contains(intern!(py, "map"))
                    .is_ok_and(|contained| contained)
            );
            assert!(
                !event_dispatchers
                    .contains(intern!(py, "damage"))
                    .is_ok_and(|contained| contained)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_with_type_not_being_subclass_of_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.add_dispatcher(&py.get_type::<Plugin>());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");

            let result =
                event_dispatchers.add_dispatcher(&py.get_type::<GameCountdownDispatcher>());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_with_type_not_being_subclass_of_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.remove_dispatcher(&py.get_type::<Plugin>().into_any());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");

            let result = event_dispatchers
                .remove_dispatcher(&py.get_type::<GameCountdownDispatcher>().into_any());

            assert!(result.is_ok());
            assert!(!event_dispatchers.__contains__("game_countdown"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_that_is_not_added(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers
                .remove_dispatcher(&py.get_type::<GameCountdownDispatcher>().into_any());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }
}

#[cfg(test)]
mod event_dispatchers_python_interface_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        exceptions::{PyKeyError, PyValueError},
        intern,
        prelude::*,
        types::PyDict,
    };
    use rstest::rstest;

    use super::{EventDispatcher, EventDispatcherManager};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                commands::CommandPriorities,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::{
                    RestoredAttribute, python_function_returning, run_all_frame_tasks,
                },
            },
        },
        prelude::*,
    };

    fn with_zmq_stats_disabled<F>(test: F)
    where
        F: FnOnce(),
    {
        let cvar_string = c"0";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let mut engine = MockEngineBuilder::default().with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            0..,
        );
        engine.run(test);
    }

    fn custom_dispatcher_type(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            c"
import shinqlx

class CustomDispatcher(shinqlx.EventDispatcher):
    name = 'custom_event'
    need_zmq_stats_enabled = False

    def __init__(self):
        super().__init__()
",
            c"",
            c"",
        )
        .and_then(|module| module.getattr("CustomDispatcher"))
        .expect("could not create the custom dispatcher")
    }

    fn registered_hooks(dispatcher: &Bound<'_, PyAny>) -> usize {
        dispatcher
            .getattr("plugins")
            .and_then(|plugins| plugins.cast_into::<PyDict>().map_err(PyErr::from))
            .map(|plugins| {
                plugins
                    .values()
                    .iter()
                    .map(|priorities| {
                        priorities
                            .try_iter()
                            .map(|lists| {
                                lists
                                    .flatten()
                                    .map(|list| list.len().unwrap_or(0))
                                    .sum::<usize>()
                            })
                            .unwrap_or(0)
                    })
                    .sum()
            })
            .expect("could not count the hooks")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_hooks_of_a_dispatcher_are_exposed_and_cleared(_pyshinqlx_setup: ()) {
        with_zmq_stats_disabled(|| {
            Python::attach(|py| {
                let dispatcher = custom_dispatcher_type(py)
                    .call0()
                    .expect("could not create the dispatcher");
                let hook = python_function_returning(py, &py.None().into_bound(py));
                dispatcher
                    .call_method1(
                        intern!(py, "add_hook"),
                        ("test_plugin", &hook, CommandPriorities::PRI_NORMAL as i32),
                    )
                    .expect("could not add the hook");

                assert!(
                    dispatcher
                        .getattr("plugins")
                        .is_ok_and(|plugins| plugins.contains("test_plugin").unwrap_or(false))
                );
                assert_eq!(registered_hooks(&dispatcher), 1);

                dispatcher
                    .cast::<EventDispatcher>()
                    .expect("not an event dispatcher")
                    .get()
                    .__clear__();

                assert_eq!(registered_hooks(&dispatcher), 0);
            })
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn adding_a_hook_while_the_hooks_are_in_use_defers_it_to_the_next_frame(_pyshinqlx_setup: ()) {
        with_zmq_stats_disabled(|| {
            Python::attach(|py| {
                let shinqlx = py
                    .import(intern!(py, "shinqlx"))
                    .expect("no shinqlx module");
                let _restore = RestoredAttribute::capture(shinqlx.as_any(), "EVENT_DISPATCHERS");
                let dispatcher = custom_dispatcher_type(py)
                    .call0()
                    .expect("could not create the dispatcher");
                let registry = PyDict::new(py);
                registry
                    .set_item("custom_event", &dispatcher)
                    .expect("could not register the dispatcher");
                shinqlx
                    .setattr(intern!(py, "EVENT_DISPATCHERS"), registry)
                    .expect("could not set the registry");
                let hook = python_function_returning(py, &py.None().into_bound(py));

                let result = {
                    let _in_use = dispatcher
                        .cast::<EventDispatcher>()
                        .expect("not an event dispatcher")
                        .get()
                        .plugins
                        .read();
                    dispatcher.call_method1(
                        intern!(py, "add_hook"),
                        ("test_plugin", &hook, CommandPriorities::PRI_NORMAL as i32),
                    )
                };

                assert!(result.is_ok());
                assert_eq!(registered_hooks(&dispatcher), 0);
                run_all_frame_tasks(py).expect("could not run the frame tasks");
                assert_eq!(registered_hooks(&dispatcher), 1);
            })
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn removing_a_hook_while_the_hooks_are_in_use_defers_it_to_the_next_frame(
        _pyshinqlx_setup: (),
    ) {
        with_zmq_stats_disabled(|| {
            Python::attach(|py| {
                let shinqlx = py
                    .import(intern!(py, "shinqlx"))
                    .expect("no shinqlx module");
                let _restore = RestoredAttribute::capture(shinqlx.as_any(), "EVENT_DISPATCHERS");
                let dispatcher = custom_dispatcher_type(py)
                    .call0()
                    .expect("could not create the dispatcher");
                let registry = PyDict::new(py);
                registry
                    .set_item("custom_event", &dispatcher)
                    .expect("could not register the dispatcher");
                shinqlx
                    .setattr(intern!(py, "EVENT_DISPATCHERS"), registry)
                    .expect("could not set the registry");
                let hook = python_function_returning(py, &py.None().into_bound(py));
                dispatcher
                    .call_method1(
                        intern!(py, "add_hook"),
                        ("test_plugin", &hook, CommandPriorities::PRI_NORMAL as i32),
                    )
                    .expect("could not add the hook");

                let result = {
                    let _in_use = dispatcher
                        .cast::<EventDispatcher>()
                        .expect("not an event dispatcher")
                        .get()
                        .plugins
                        .read();
                    dispatcher.call_method1(
                        intern!(py, "remove_hook"),
                        ("test_plugin", &hook, CommandPriorities::PRI_NORMAL as i32),
                    )
                };

                assert!(result.is_ok());
                assert_eq!(registered_hooks(&dispatcher), 1);
                run_all_frame_tasks(py).expect("could not run the frame tasks");
                assert_eq!(registered_hooks(&dispatcher), 0);
            })
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn the_dispatcher_manager_is_managed_through_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let manager =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            let dispatcher_type = custom_dispatcher_type(py);

            manager
                .call_method1("add_dispatcher", (&dispatcher_type,))
                .expect("could not add the dispatcher");

            assert!(manager.contains("custom_event").unwrap_or(false));
            assert!(!manager.contains("other_event").unwrap_or(true));
            assert!(manager.get_item("custom_event").is_ok());
            assert!(
                manager
                    .get_item("other_event")
                    .is_err_and(|err| err.is_instance_of::<PyKeyError>(py))
            );
            assert!(
                manager
                    .getattr("_dispatchers")
                    .and_then(|dispatchers| dispatchers.len())
                    .is_ok_and(|count| count == 1)
            );
            assert!(
                manager
                    .call_method1("add_dispatcher", (&dispatcher_type,))
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );

            manager
                .call_method1("remove_dispatcher", (&dispatcher_type,))
                .expect("could not remove the dispatcher");
            assert!(!manager.contains("custom_event").unwrap_or(true));

            manager
                .call_method1("add_dispatcher", (&dispatcher_type,))
                .expect("could not add the dispatcher");
            manager
                .call_method1("remove_dispatcher_by_name", ("custom_event",))
                .expect("could not remove the dispatcher by name");
            assert!(!manager.contains("custom_event").unwrap_or(true));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn clearing_the_dispatcher_manager_drops_its_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let manager =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            manager
                .call_method1("add_dispatcher", (custom_dispatcher_type(py),))
                .expect("could not add the dispatcher");

            manager.get().__clear__();

            assert!(!manager.contains("custom_event").unwrap_or(true));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn removing_a_dispatcher_while_the_dispatchers_are_in_use_defers_it_to_the_next_frame(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let shinqlx = py
                .import(intern!(py, "shinqlx"))
                .expect("no shinqlx module");
            let _restore = RestoredAttribute::capture(shinqlx.as_any(), "EVENT_DISPATCHERS");
            let manager =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            shinqlx
                .setattr(intern!(py, "EVENT_DISPATCHERS"), &manager)
                .expect("could not set the registry");
            manager
                .call_method1("add_dispatcher", (custom_dispatcher_type(py),))
                .expect("could not add the dispatcher");

            let result = {
                let _in_use = manager.get().dispatchers.read();
                manager.call_method1("remove_dispatcher_by_name", ("custom_event",))
            };

            assert!(result.is_ok());
            assert!(manager.contains("custom_event").unwrap_or(false));
            run_all_frame_tasks(py).expect("could not run the frame tasks");
            assert!(!manager.contains("custom_event").unwrap_or(true));
        });
    }
}
