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
    pub(crate) use super::{log_unexpected_return_value, EventDispatcher};

    pub(crate) use super::super::{log_exception, pyshinqlx_get_logger, PythonReturnCodes};

    pub(crate) use pyo3::intern;
    pub(crate) use pyo3::prelude::*;
}

use prelude::*;

use super::{commands::CommandPriorities, embed::pyshinqlx_get_cvar};

pub(crate) use chat_event_dispatcher::ChatEventDispatcher;
pub(crate) use client_command_dispatcher::ClientCommandDispatcher;
pub(crate) use command_dispatcher::CommandDispatcher;
pub(crate) use console_print_dispatcher::ConsolePrintDispatcher;
pub(crate) use damage_dispatcher::DamageDispatcher;
pub(crate) use death_dispatcher::DeathDispatcher;
pub(crate) use frame_event_dispatcher::FrameEventDispatcher;
pub(crate) use game_countdown_dispatcher::GameCountdownDispatcher;
pub(crate) use game_end_dispatcher::GameEndDispatcher;
pub(crate) use game_start_dispatcher::GameStartDispatcher;
pub(crate) use kamikaze_explode_dispatcher::KamikazeExplodeDispatcher;
pub(crate) use kamikaze_use_dispatcher::KamikazeUseDispatcher;
pub(crate) use kill_dispatcher::KillDispatcher;
pub(crate) use map_dispatcher::MapDispatcher;
pub(crate) use new_game_dispatcher::NewGameDispatcher;
pub(crate) use player_connect_dispatcher::PlayerConnectDispatcher;
pub(crate) use player_disconnect_dispatcher::PlayerDisconnectDispatcher;
pub(crate) use player_loaded_dispatcher::PlayerLoadedDispatcher;
pub(crate) use player_spawn_dispatcher::PlayerSpawnDispatcher;
pub(crate) use round_countdown_dispatcher::RoundCountdownDispatcher;
pub(crate) use round_end_dispatcher::RoundEndDispatcher;
pub(crate) use round_start_dispatcher::RoundStartDispatcher;
pub(crate) use server_command_dispatcher::ServerCommandDispatcher;
pub(crate) use set_configstring_dispatcher::SetConfigstringDispatcher;
pub(crate) use stats_dispatcher::StatsDispatcher;
pub(crate) use team_switch_attempt_dispatcher::TeamSwitchAttemptDispatcher;
pub(crate) use team_switch_dispatcher::TeamSwitchDispatcher;
pub(crate) use unload_dispatcher::UnloadDispatcher;
pub(crate) use userinfo_dispatcher::UserinfoDispatcher;
pub(crate) use vote_called_dispatcher::VoteCalledDispatcher;
pub(crate) use vote_dispatcher::VoteDispatcher;
pub(crate) use vote_ended_dispatcher::VoteEndedDispatcher;
pub(crate) use vote_started_dispatcher::VoteStartedDispatcher;

use pyo3::{
    exceptions::{PyAssertionError, PyValueError},
    types::PyTuple,
};

use core::ops::Deref;
use itertools::Itertools;

pub(crate) fn log_unexpected_return_value(
    py: Python<'_>,
    event_name: &str,
    result: &PyObject,
    handler: &PyObject,
) {
    if let Ok(logger) = pyshinqlx_get_logger(py, None) {
        match handler.getattr(py, intern!(py, "__name__")) {
            Err(e) => log_exception(py, e),
            Ok(handler_name) => {
                if let Err(e) = logger.call_method1(
                    intern!(py, "warning"),
                    (
                        "Handler '%s' returned unknown value '%s' for event '%s'",
                        handler_name,
                        result,
                        event_name,
                    ),
                ) {
                    log_exception(py, e);
                };
            }
        };
    }
}

#[pyclass(name = "EventDispatcher", module = "_events", subclass)]
pub(crate) struct EventDispatcher {
    name: String,
    need_zmq_stats_enabled: bool,
    plugins: Vec<(PyObject, [Vec<PyObject>; 5])>,
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
            name: "".into(),
            need_zmq_stats_enabled: false,
            plugins: Vec::new(),
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
        NO_DEBUG.into_iter().collect_tuple().unwrap_or_default()
    }

    #[new]
    pub(crate) fn py_new(_py: Python<'_>) -> Self {
        Self::default()
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
    pub(crate) fn dispatch(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        args: Bound<'_, PyTuple>,
    ) -> PyObject {
        if !NO_DEBUG.contains(&slf.name.as_str()) {
            if let Ok(logger) = pyshinqlx_get_logger(py, None) {
                let mut dbgstr = format!("{}{}", slf.name, &args);
                if dbgstr.len() > 100 {
                    dbgstr.truncate(99);
                    dbgstr.push(')');
                }
                if let Err(e) = logger.call_method1(intern!(py, "debug"), (dbgstr,)) {
                    log_exception(py, e);
                };
            }
        }

        let mut return_value = true.into_py(py);

        for i in 0..5 {
            for (_, handlers) in &slf.plugins.clone() {
                for handler in &handlers[i] {
                    let handler_args = PyTuple::new_bound(py, &args);
                    match handler.call1(py, handler_args) {
                        Err(e) => {
                            log_exception(py, e);
                            continue;
                        }
                        Ok(res) => {
                            let res_i32 = res.extract::<PythonReturnCodes>(py);
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
                                return true.into_py(py);
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                return_value = false.into_py(py);
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return false.into_py(py);
                            }

                            match Self::handle_return(slf.deref(), py, handler.into_py(py), res) {
                                Err(e) => {
                                    log_exception(py, e);
                                    continue;
                                }
                                Ok(return_handler) => {
                                    if !return_handler.is_none(py) {
                                        return return_handler;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        return_value.clone()
    }

    /// Handle an unknown return value. If this returns anything but None,
    /// it will stop execution of the event and pass the return value on
    /// to the C-level handlers. This method can be useful to override,
    /// because of the fact that you can edit the arguments that will be
    /// passed on to any handler after whatever handler returned *value*
    /// by editing *self.args*. Furthermore, *self.return_value*
    /// is the return value that will be sent to the C-level handler if the
    /// event isn't stopped later along the road.
    pub(crate) fn handle_return(
        &self,
        py: Python<'_>,
        handler: PyObject,
        value: PyObject,
    ) -> PyResult<PyObject> {
        log_unexpected_return_value(py, &self.name, &value, &handler);
        Ok(py.None())
    }

    /// Hook the event, making the handler get called with relevant arguments
    /// whenever the event is takes place.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32))]
    fn add_hook(
        &mut self,
        py: Python<'_>,
        plugin: PyObject,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        if !(0i32..5i32).contains(&priority) {
            let error_description = format!("'{}' is an invalid priority level.", priority);
            return Err(PyValueError::new_err(error_description));
        }

        let zmq_enabled_cvar = pyshinqlx_get_cvar(py, "zmq_stats_enable")?;
        let zmq_enabled = zmq_enabled_cvar.is_some_and(|value| value != "0");
        if self.need_zmq_stats_enabled && !zmq_enabled {
            let error_description = format!(
                "{} hook requires zmq_stats_enabled cvar to have nonzero value",
                self.name.clone()
            );
            return Err(PyAssertionError::new_err(error_description));
        }

        let Some(plugin_commands) = self.plugins.iter_mut().find(|(added_plugin, _)| {
            let is_eq = added_plugin.call_method1(py, "__eq__", (&plugin,));
            is_eq.is_ok_and(|result| result.is_truthy(py).is_ok_and(|value| value))
        }) else {
            let mut new_commands = (plugin.into_py(py), [vec![], vec![], vec![], vec![], vec![]]);
            new_commands.1[priority as usize].push(handler);
            self.plugins.push(new_commands);
            return Ok(());
        };

        if plugin_commands.1.iter().any(|registered_commands| {
            registered_commands.iter().any(|hook| {
                let handler_eq = hook.call_method1(py, "__eq__", (&handler,));
                handler_eq.is_ok_and(|result| result.is_truthy(py).is_ok_and(|value| value))
            })
        }) {
            return Err(PyValueError::new_err(
                "The event has already been hooked with the same handler and priority.",
            ));
        }

        plugin_commands.1[priority as usize].push(handler);
        Ok(())
    }

    /// Removes a previously hooked event.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32))]
    fn remove_hook(
        &mut self,
        py: Python<'_>,
        plugin: PyObject,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let Some(plugin_commands) = self.plugins.iter_mut().find(|(added_plugin, _)| {
            let is_eq = added_plugin.call_method1(py, "__eq__", (&plugin,));
            is_eq.is_ok_and(|result| result.is_truthy(py).is_ok_and(|value| value))
        }) else {
            return Err(PyValueError::new_err(
                "The event has not been hooked with the handler provided",
            ));
        };

        if !plugin_commands.1[priority as usize].iter().all(|item| {
            item.call_method1(py, "__ne__", (&handler,))
                .is_ok_and(|value| value.is_truthy(py).is_ok_and(|bool_value| bool_value))
        }) {
            return Err(PyValueError::new_err(
                "The event has not been hooked with the handler provided",
            ));
        }

        plugin_commands.1[priority as usize].retain(|item| {
            item.call_method1(py, "__ne__", (&handler,))
                .is_ok_and(|value| value.is_truthy(py).is_ok_and(|bool_value| bool_value))
        });

        Ok(())
    }
}
