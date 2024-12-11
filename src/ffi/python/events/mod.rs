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
    pub(crate) use super::{
        EventDispatcher, EventDispatcherMethods, dispatcher_debug_log, log_unexpected_return_value,
    };

    pub(crate) use super::super::{PythonReturnCodes, log_exception, pyshinqlx_get_logger};

    pub(crate) use pyo3::intern;
    pub(crate) use pyo3::prelude::*;
}

use prelude::*;

use super::{commands::CommandPriorities, get_cvar};

pub(crate) use chat_event_dispatcher::{ChatEventDispatcher, ChatEventDispatcherMethods};
pub(crate) use client_command_dispatcher::{
    ClientCommandDispatcher, ClientCommandDispatcherMethods,
};
pub(crate) use command_dispatcher::{CommandDispatcher, CommandDispatcherMethods};
pub(crate) use console_print_dispatcher::{ConsolePrintDispatcher, ConsolePrintDispatcherMethods};
pub(crate) use damage_dispatcher::{DamageDispatcher, DamageDispatcherMethods};
#[allow(unused_imports)]
pub(crate) use death_dispatcher::{DeathDispatcher, DeathDispatcherMethods};
pub(crate) use frame_event_dispatcher::{FrameEventDispatcher, FrameEventDispatcherMethods};
pub(crate) use game_countdown_dispatcher::{
    GameCountdownDispatcher, GameCountdownDispatcherMethods,
};
#[allow(unused_imports)]
pub(crate) use game_end_dispatcher::{GameEndDispatcher, GameEndDispatcherMethods};
#[allow(unused_imports)]
pub(crate) use game_start_dispatcher::{GameStartDispatcher, GameStartDispatcherMethods};
pub(crate) use kamikaze_explode_dispatcher::{
    KamikazeExplodeDispatcher, KamikazeExplodeDispatcherMethods,
};
pub(crate) use kamikaze_use_dispatcher::{KamikazeUseDispatcher, KamikazeUseDispatcherMethods};
#[allow(unused_imports)]
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
pub(crate) use round_countdown_dispatcher::{
    RoundCountdownDispatcher, RoundCountdownDispatcherMethods,
};
#[allow(unused_imports)]
pub(crate) use round_end_dispatcher::{RoundEndDispatcher, RoundEndDispatcherMethods};
pub(crate) use round_start_dispatcher::{RoundStartDispatcher, RoundStartDispatcherMethods};
pub(crate) use server_command_dispatcher::{
    ServerCommandDispatcher, ServerCommandDispatcherMethods,
};
pub(crate) use set_configstring_dispatcher::{
    SetConfigstringDispatcher, SetConfigstringDispatcherMethods,
};
#[allow(unused_imports)]
pub(crate) use stats_dispatcher::{StatsDispatcher, StatsDispatcherMethods};
pub(crate) use team_switch_attempt_dispatcher::{
    TeamSwitchAttemptDispatcher, TeamSwitchAttemptDispatcherMethods,
};
#[allow(unused_imports)]
pub(crate) use team_switch_dispatcher::{TeamSwitchDispatcher, TeamSwitchDispatcherMethods};
pub(crate) use unload_dispatcher::{UnloadDispatcher, UnloadDispatcherMethods};
pub(crate) use userinfo_dispatcher::{UserinfoDispatcher, UserinfoDispatcherMethods};
pub(crate) use vote_called_dispatcher::{VoteCalledDispatcher, VoteCalledDispatcherMethods};
pub(crate) use vote_dispatcher::{VoteDispatcher, VoteDispatcherMethods};
pub(crate) use vote_ended_dispatcher::{VoteEndedDispatcher, VoteEndedDispatcherMethods};
pub(crate) use vote_started_dispatcher::{VoteStartedDispatcher, VoteStartedDispatcherMethods};

use pyo3::{
    PyTraverseError, PyVisit,
    exceptions::{PyAssertionError, PyKeyError, PyValueError},
    types::{PyBool, PyDict, PyTuple, PyType},
};

use itertools::Itertools;
use pyo3::exceptions::PyAttributeError;
use pyo3::types::IntoPyDict;

fn try_dispatcher_debug_log(py: Python<'_>, debug_str: &str) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let debug_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "DEBUG")))?;

        let mut dbgstr = debug_str.to_string();
        if dbgstr.len() > 100 {
            dbgstr.truncate(99);
            dbgstr.push(')');
        }
        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    debug_level,
                    intern!(py, ""),
                    -1,
                    dbgstr,
                    py.None(),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    Ok(())
}

pub(crate) fn dispatcher_debug_log(py: Python<'_>, debug_str: &str) {
    if let Err(e) = try_dispatcher_debug_log(py, debug_str) {
        log_exception(py, &e);
    }
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
    if let Err(e) = try_log_unexpected_return_value(py, event_name, result, handler) {
        log_exception(py, &e);
    }
}

#[pyclass(name = "EventDispatcher", module = "_events", subclass, frozen)]
pub(crate) struct EventDispatcher {
    plugins: parking_lot::RwLock<Vec<(String, [Vec<PyObject>; 5])>>,
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
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "(plugin, handler, priority=PRI_NORMAL)")]
    fn add_hook(
        slf: &Bound<'_, Self>,
        plugin: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.add_hook(plugin, handler, priority)
    }

    /// Removes a previously hooked event.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "(plugin, handler, priority=PRI_NORMAL)")]
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
        self.try_borrow()
            .ok()
            .and_then(|event_dispatcher| {
                event_dispatcher.plugins.try_read().and_then(|plugins| {
                    plugins
                        .iter()
                        .map(|(plugin_name, hooks)| {
                            (
                                plugin_name.clone(),
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
                        .collect::<Vec<(String, Vec<Vec<PyObject>>)>>()
                        .into_py_dict(self.py())
                        .ok()
                })
            })
            .unwrap_or(PyDict::new(self.py()))
    }

    fn dispatch(&self, args: &Bound<'py, PyTuple>) -> Bound<'py, PyAny> {
        let Ok(event_dispatcher) = self.try_borrow() else {
            return self.py().None().into_bound(self.py());
        };
        let Ok(dispatcher_name) = self
            .get_type()
            .getattr(intern!(self.py(), "name"))
            .and_then(|py_dispatcher_name| py_dispatcher_name.extract::<String>())
        else {
            return self.py().None().into_bound(self.py());
        };
        if !NO_DEBUG.contains(&dispatcher_name.as_str()) {
            let dbgstr = format!("{}{}", dispatcher_name, args);
            dispatcher_debug_log(self.py(), &dbgstr);
        }

        let mut return_value = PyBool::new(self.py(), true).to_owned().into_any().unbind();

        let plugins = event_dispatcher.plugins.read();
        for i in 0..5 {
            for (_, handlers) in &*plugins {
                for handler in &handlers[i] {
                    match handler.call1(self.py(), args) {
                        Err(e) => {
                            log_exception(self.py(), &e);
                            continue;
                        }
                        Ok(res) => {
                            let res_i32 = res.extract::<PythonReturnCodes>(self.py());
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_NONE)
                            {
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP)
                            {
                                return PyBool::new(self.py(), true).to_owned().into_any();
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                return_value =
                                    PyBool::new(self.py(), false).to_owned().into_any().unbind();
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return PyBool::new(self.py(), false).to_owned().into_any();
                            }

                            match self.call_method1(
                                intern!(self.py(), "handle_return"),
                                (handler.bind(self.py()), res),
                            ) {
                                Err(e) => {
                                    log_exception(self.py(), &e);
                                    continue;
                                }
                                Ok(return_handler) => {
                                    if !return_handler.is_none() {
                                        return return_handler;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        return_value.bind(self.py()).clone()
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
            let error_description = format!("'{}' is an invalid priority level.", priority);
            return Err(PyValueError::new_err(error_description));
        }

        let event_dispatcher = self
            .try_borrow()
            .map_err(|_| PyValueError::new_err("could not borrow event_dispatcher"))?;
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
            let error_description = format!(
                "{} hook requires zmq_stats_enabled cvar to have nonzero value",
                dispatcher_name
            );
            return Err(PyAssertionError::new_err(error_description));
        }

        match event_dispatcher.plugins.try_write() {
            None => {
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
                    new_hooks.1[priority as usize].push(handler.clone().unbind());
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
                    return Err(PyValueError::new_err(
                        "The event has already been hooked with the same handler and priority.",
                    ));
                }

                plugin_hooks.1[priority as usize].push(handler.clone().unbind());
            }
        }
        Ok(())
    }

    fn remove_hook(&self, plugin: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        let event_dispatcher = self
            .try_borrow()
            .map_err(|_| PyValueError::new_err("could not borrow event_dispatcher"))?;
        let dispatcher_name = self
            .get_type()
            .getattr(intern!(self.py(), "name"))
            .and_then(|value| value.extract::<String>())
            .map_err(|_| {
                PyAttributeError::new_err(
                    "Cannot remove a hook from an event dispatcher with no name.",
                )
            })?;
        match event_dispatcher.plugins.try_write() {
            None => {
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
                    return Err(PyValueError::new_err(
                        "The event has not been hooked with the handler provided",
                    ));
                };

                if !plugin_hooks.1[priority as usize]
                    .iter()
                    .any(|item| item.bind(self.py()).eq(handler).unwrap_or(true))
                {
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
    use crate::ffi::c::prelude::{CVar, CVarBuilder, cvar_t};
    use crate::ffi::python::commands::CommandPriorities;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::*;

    use pyo3::exceptions::{PyAssertionError, PyAttributeError, PyValueError};
    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyDict, PyTuple};

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
        .getattr("CustomDispatcher")
        .expect("this should not happen")
        .call0()
        .expect("this should not happen")
    }

    fn custom_hook(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            cr#"
def custom_hook(*args, **kwargs):
    pass
        "#,
            c"",
            c"",
        )
        .expect("this should not happen")
        .getattr("custom_hook")
        .expect("this should not happen")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_plugins_when_none_are_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher = custom_dispatcher(py);

            let result = dispatcher.getattr("plugins");
            assert!(
                result.is_ok_and(|value| value
                    .downcast::<PyDict>()
                    .is_ok_and(|dict| dict.is_empty()))
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);
                    dispatcher
                        .call_method1(
                            "add_hook",
                            (
                                "prio5_plugin",
                                custom_hook(py),
                                CommandPriorities::PRI_LOWEST as i32,
                            ),
                        )
                        .expect("this should not happen");
                    dispatcher
                        .call_method1(
                            "add_hook",
                            (
                                "prio4_plugin",
                                custom_hook(py),
                                CommandPriorities::PRI_LOW as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            "add_hook",
                            (
                                "prio3_plugin",
                                custom_hook(py),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            "add_hook",
                            (
                                "prio2_plugin",
                                custom_hook(py),
                                CommandPriorities::PRI_HIGH as i32,
                            ),
                        )
                        .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            "add_hook",
                            (
                                "prio1_plugin",
                                custom_hook(py),
                                CommandPriorities::PRI_HIGHEST as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result = dispatcher.getattr("plugins");
                    assert!(result.is_ok_and(|value| {
                        value.downcast::<PyDict>().is_ok_and(|dict| dict.len() == 5)
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher = custom_dispatcher(py);

            let result = dispatcher.call_method1(intern!(py, "dispatch"), PyTuple::empty(py));
            assert!(result.is_ok_and(|value| {
                value
                    .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let throws_exception_hook = PyModule::from_code(
                        py,
                        cr#"
def throws_exception_hook(*args, **kwargs):
    raise ValueError("asdf")
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("throws_exception_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_none_hook = PyModule::from_code(
                        py,
                        cr#"
def returns_none_hook(*args, **kwargs):
    return None
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_none_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_none_hook = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

def returns_none_hook(*args, **kwargs):
    return shinqlx.RET_NONE
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_none_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_hook = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

def returns_stop_hook(*args, **kwargs):
    return shinqlx.RET_STOP
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_stop_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_event_hook = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

def returns_stop_event_hook(*args, **kwargs):
    return shinqlx.RET_STOP_EVENT
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_stop_event_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_stop_all_hook = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

def returns_stop_all_hook(*args, **kwargs):
    return shinqlx.RET_STOP_ALL
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_stop_all_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let returns_string_hook = PyModule::from_code(
                        py,
                        cr#"
def returns_string_hook(*args, **kwargs):
    return "return string"
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_string_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);
                    let return_handler = PyModule::from_code(
                        py,
                        cr#"
def handle_return(*args, **kwargs):
    raise ValueError("return_handler default exception")
                "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("handle_return")
                    .expect("this should not happen");
                    dispatcher
                        .setattr("handle_return", return_handler)
                        .expect("this should not happen");

                    let returns_string_hook = PyModule::from_code(
                        py,
                        cr#"
def returns_string_hook(*args, **kwargs):
    return "return string"
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_string_hook")
                    .expect("this should not happen");

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
                            .downcast::<PyBool>()
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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);
                    let return_handler = PyModule::from_code(
                        py,
                        cr#"
def handle_return(*args, **kwargs):
    return "return_handler string return"
                "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("handle_return")
                    .expect("this should not happen");
                    dispatcher
                        .setattr("handle_return", return_handler)
                        .expect("this should not happen");

                    let returns_string_hook = PyModule::from_code(
                        py,
                        cr#"
def returns_string_hook(*args, **kwargs):
    return "return string"
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_string_hook")
                    .expect("this should not happen");

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
        Python::with_gil(|py| {
            let dispatcher = custom_dispatcher(py);

            let default_hook = PyModule::from_code(
                py,
                cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr("default_hook")
            .expect("this should not happen");

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
        Python::with_gil(|py| {
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
            .getattr("NamelessDispatcher")
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = PyModule::from_code(
                py,
                cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr("default_hook")
            .expect("this should not happen");

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
        Python::with_gil(|py| {
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
            .getattr("ZmqLessDispatcher")
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = PyModule::from_code(
                py,
                cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr("default_hook")
            .expect("this should not happen");

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
                Python::with_gil(|py| {
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
                    .getattr("ZmqEnabledDispatcher")
                    .expect("this should not happen")
                    .call0()
                    .expect("this should not happen");

                    let default_hook = PyModule::from_code(
                        py,
                        cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("default_hook")
                    .expect("this should not happen");

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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let default_hook = PyModule::from_code(
                        py,
                        cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("default_hook")
                    .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                default_hook.clone().unbind(),
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
        Python::with_gil(|py| {
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
            .getattr("NamelessDispatcher")
            .expect("this should not happen")
            .call0()
            .expect("this should not happen");

            let default_hook = PyModule::from_code(
                py,
                cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr("default_hook")
            .expect("this should not happen");

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
        Python::with_gil(|py| {
            let dispatcher = custom_dispatcher(py);

            let default_hook = PyModule::from_code(
                py,
                cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                c"",
                c"",
            )
            .expect("this should not happen")
            .getattr("default_hook")
            .expect("this should not happen");

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
                Python::with_gil(|py| {
                    let dispatcher = custom_dispatcher(py);

                    let default_hook = PyModule::from_code(
                        py,
                        cr#"
def default_hook(*args, **kwargs):
    pass
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("default_hook")
                    .expect("this should not happen");

                    dispatcher
                        .call_method1(
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                default_hook.clone().unbind(),
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
    dispatchers: parking_lot::RwLock<Vec<(String, PyObject)>>,
}

#[pymethods]
impl EventDispatcherManager {
    #[new]
    fn py_new(py: Python<'_>) -> Self {
        py.allow_threads(Self::default)
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

    fn remove_dispatcher_by_name(slf: &Bound<'_, Self>, dispatcher_name: &str) -> PyResult<()> {
        slf.remove_dispatcher_by_name(dispatcher_name)
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
        self.borrow()
            .dispatchers
            .read()
            .iter()
            .any(|(event_name, _)| key == event_name)
    }

    fn __getitem__(&self, key: &str) -> PyResult<Bound<'py, PyAny>> {
        self.borrow()
            .dispatchers
            .read()
            .iter()
            .find(|(event_name, _)| key == event_name)
            .map_or_else(
                || {
                    let key_error = format!("'{}'", key);
                    Err(PyKeyError::new_err(key_error))
                },
                |(_, dispatcher)| Ok(dispatcher.bind(self.py()).to_owned()),
            )
    }

    fn get_dispatchers(&self) -> PyResult<Bound<'py, PyDict>> {
        self.borrow()
            .dispatchers
            .read()
            .iter()
            .map(|(dispatcher_name, dispatch_function)| {
                (
                    dispatcher_name.clone(),
                    dispatch_function.bind(self.py()).as_unbound(),
                )
            })
            .collect::<Vec<(String, &PyObject)>>()
            .into_py_dict(self.py())
    }

    fn add_dispatcher(&self, dispatcher: &Bound<'py, PyType>) -> PyResult<()> {
        if !dispatcher
            .is_subclass_of::<EventDispatcher>()
            .unwrap_or(false)
        {
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher not based on EventDispatcher.",
            ));
        }

        let dispatcher_name_str = dispatcher
            .getattr(intern!(self.py(), "name"))
            .and_then(|dispatcher_name_attr| dispatcher_name_attr.extract::<String>())
            .map_err(|_| PyValueError::new_err("Cannot add an event dispatcher with no name."))?;
        if self.__contains__(&dispatcher_name_str) {
            return Err(PyValueError::new_err("Event name already taken."));
        }

        self.borrow()
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
            return Err(PyValueError::new_err("Event name not found."));
        }

        match self.borrow().dispatchers.try_write() {
            None => {
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
    use super::{
        EventDispatcherManager, EventDispatcherManagerMethods, GameCountdownDispatcher,
        GameEndDispatcher, GameStartDispatcher,
    };
    use pyo3::exceptions::PyValueError;

    use crate::ffi::python::plugin::Plugin;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    use rstest::*;

    use pyo3::exceptions::PyKeyError;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn event_dispatcher_manager_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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

            let result = py.import("gc").and_then(|gc| gc.call_method0("collect"));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_when_no_dispatchers_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = EventDispatcherManager::get_dispatchers(&event_dispatchers);
            assert!(result.is_ok_and(|dict| dict.is_empty()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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

            let result = EventDispatcherManager::get_dispatchers(&event_dispatchers)
                .expect("this should not happen");
            assert!(
                result
                    .get_item("game_countdown")
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameCountdownDispatcher>()
                    }))
            );
            assert!(
                result
                    .get_item("game_start")
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameStartDispatcher>()
                    }))
            );
            assert!(
                result
                    .get_item("game_end")
                    .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                        dispatcher.is_instance_of::<GameEndDispatcher>()
                    }))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_item_from_empty_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.__getitem__("game_start");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_item_for_existing_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");
            event_dispatchers
                .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");

            let result = event_dispatchers.__getitem__("game_start");
            assert!(
                result.is_ok_and(|dispatcher| dispatcher.is_instance_of::<GameStartDispatcher>())
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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

            assert!(event_dispatchers.__contains__("game_countdown"));
            assert!(event_dispatchers.__contains__("game_start"));
            assert!(event_dispatchers.__contains__("game_end"));
            assert!(!event_dispatchers.__contains__("map"));
            assert!(!event_dispatchers.__contains__("damage"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_with_type_not_being_subclass_of_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.add_dispatcher(&py.get_type::<Plugin>());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers.remove_dispatcher(&py.get_type::<Plugin>().into_any());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let event_dispatchers =
                Bound::new(py, EventDispatcherManager::py_new(py)).expect("this should not happen");

            let result = event_dispatchers
                .remove_dispatcher(&py.get_type::<GameCountdownDispatcher>().into_any());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }
}
