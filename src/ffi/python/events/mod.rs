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
    pub(crate) use super::{dispatcher_debug_log, log_unexpected_return_value, EventDispatcher};

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
    exceptions::{PyAssertionError, PyKeyError, PyValueError},
    types::{PyDict, PyTuple, PyType},
};

use core::ops::Deref;
use itertools::Itertools;
use pyo3::types::IntoPyDict;

fn try_dispatcher_debug_log(py: Python<'_>, debug_str: &str) -> PyResult<()> {
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;

    let logger = pyshinqlx_get_logger(py, None)?;

    let mut dbgstr = debug_str.to_string();
    if dbgstr.len() > 100 {
        dbgstr.truncate(99);
        dbgstr.push(')');
    }
    let log_record = logger.call_method(
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
        Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

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
    result: &PyObject,
    handler: &PyObject,
) -> PyResult<()> {
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let warning_level = logging_module.getattr(intern!(py, "WARNING"))?;

    let logger = pyshinqlx_get_logger(py, None)?;
    let handler_name = handler.getattr(py, intern!(py, "__name__"))?;

    let log_record = logger.call_method(
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
        Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    Ok(())
}

pub(crate) fn log_unexpected_return_value(
    py: Python<'_>,
    event_name: &str,
    result: &PyObject,
    handler: &PyObject,
) {
    if let Err(e) = try_log_unexpected_return_value(py, event_name, result, handler) {
        log_exception(py, &e);
    }
}

#[pyclass(name = "EventDispatcher", module = "_events", subclass)]
pub(crate) struct EventDispatcher {
    name: String,
    need_zmq_stats_enabled: bool,
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
            name: "".to_string(),
            need_zmq_stats_enabled: false,
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
        NO_DEBUG.into_iter().collect_tuple().unwrap_or_default()
    }

    #[new]
    pub(crate) fn py_new(_py: Python<'_>) -> Self {
        Self::default()
    }

    #[getter(plugins)]
    fn get_plugins<'py>(&'py self, py: Python<'py>) -> Bound<'py, PyDict> {
        let plugins = self.plugins.read();
        plugins.clone().into_py_dict_bound(py)
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
            let dbgstr = format!("{}{}", slf.name, &args);
            dispatcher_debug_log(py, &dbgstr);
        }

        let mut return_value = true.into_py(py);

        let plugins = slf.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.clone() {
                for handler in &handlers[i] {
                    let handler_args = PyTuple::new_bound(py, &args);
                    match handler.call1(py, handler_args) {
                        Err(e) => {
                            log_exception(py, &e);
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
                                    log_exception(py, &e);
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
        &self,
        py: Python<'_>,
        plugin: &str,
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

        let Some(mut plugins) = self.plugins.try_write() else {
            let add_hook_func = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def add_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].add_hook(plugin, handler, priority)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "add_hook"))?;

            add_hook_func.call1((&self.name, plugin, handler, priority))?;
            return Ok(());
        };
        let Some(plugin_hooks) = plugins
            .iter_mut()
            .find(|(added_plugin, _)| added_plugin == plugin)
        else {
            let mut new_commands = (plugin.to_string(), [vec![], vec![], vec![], vec![], vec![]]);
            new_commands.1[priority as usize].push(handler);
            plugins.push(new_commands);
            return Ok(());
        };

        if plugin_hooks.1.iter().any(|registered_commands| {
            registered_commands
                .iter()
                .any(|hook| hook.bind(py).eq(handler.bind(py)).unwrap_or(false))
        }) {
            return Err(PyValueError::new_err(
                "The event has already been hooked with the same handler and priority.",
            ));
        }

        plugin_hooks.1[priority as usize].push(handler);
        Ok(())
    }

    /// Removes a previously hooked event.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32))]
    fn remove_hook(
        &self,
        py: Python<'_>,
        plugin: &str,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let Some(mut plugins) = self.plugins.try_write() else {
            let remove_hook_func = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def remove_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].remove_hook(plugin, handler, priority)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "remove_hook"))?;

            remove_hook_func.call1((&self.name, plugin, handler, priority))?;
            return Ok(());
        };
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
            .any(|item| item.bind(py).eq(handler.bind(py)).unwrap_or(true))
        {
            return Err(PyValueError::new_err(
                "The event has not been hooked with the handler provided",
            ));
        }

        plugin_hooks.1[priority as usize]
            .retain(|item| item.bind(py).ne(handler.bind(py)).unwrap_or(true));

        Ok(())
    }
}

/// Holds all the event dispatchers and provides a way to access the dispatcher
/// instances by accessing it like a dictionary using the event name as a key.
/// Only one dispatcher can be used per event.
#[pyclass(name = "EventDispatcherManager", module = "_events")]
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

    #[getter(_dispatchers)]
    fn get_dispatchers<'py>(&'py self, py: Python<'py>) -> Bound<'py, PyDict> {
        let dispatchers = self.dispatchers.read();
        dispatchers.clone().into_py_dict_bound(py)
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        let dispatchers = self.dispatchers.read();
        dispatchers
            .iter()
            .find_map(|(event_name, dispatcher)| {
                if key != event_name {
                    None
                } else {
                    Some(dispatcher)
                }
            })
            .map_or_else(
                || {
                    let key_error = format!("'{}'", key);
                    Err(PyKeyError::new_err(key_error))
                },
                |dispatcher| Ok(dispatcher.into_py(py)),
            )
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> bool {
        py.allow_threads(|| {
            let dispatchers = self.dispatchers.read();
            dispatchers
                .iter()
                .find_map(|(event_name, dispatcher)| {
                    if key != event_name {
                        None
                    } else {
                        Some(dispatcher)
                    }
                })
                .is_some()
        })
    }

    pub(crate) fn add_dispatcher(
        &self,
        py: Python<'_>,
        dispatcher: Bound<'_, PyType>,
    ) -> PyResult<()> {
        let Ok(dispatcher_name_attr) = dispatcher.getattr("name") else {
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher with no name.",
            ));
        };
        let Ok(dispatcher_name_str) = dispatcher_name_attr.extract::<String>() else {
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher with no name.",
            ));
        };
        if self.__contains__(py, &dispatcher_name_str) {
            return Err(PyValueError::new_err("Event name already taken."));
        }

        if !dispatcher
            .is_subclass_of::<EventDispatcher>()
            .unwrap_or(false)
        {
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher not based on EventDispatcher.",
            ));
        }

        let mut dispatchers = self.dispatchers.write();
        dispatchers.push((dispatcher_name_str, dispatcher.call0()?.unbind()));

        Ok(())
    }

    fn remove_dispatcher(&self, py: Python<'_>, dispatcher: PyObject) -> PyResult<()> {
        let Ok(dispatcher_name_attr) = dispatcher.getattr(py, "name") else {
            return Err(PyValueError::new_err(
                "Cannot remove an event dispatcher with no name.",
            ));
        };
        let Ok(dispatcher_name_str) = dispatcher_name_attr.extract::<String>(py) else {
            return Err(PyValueError::new_err(
                "Cannot remove an event dispatcher with no name.",
            ));
        };

        self.remove_dispatcher_by_name(py, &dispatcher_name_str)
    }

    fn remove_dispatcher_by_name(&self, py: Python<'_>, dispatcher_name: &str) -> PyResult<()> {
        if !self.__contains__(py, dispatcher_name) {
            return Err(PyValueError::new_err("Event name not found."));
        }

        let Some(mut dispatchers) = self.dispatchers.try_write() else {
            let remove_dispatcher_by_name_func = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def remove_dispatcher_by_name(dispatcher_name):
    shinqlx.EVENT_DISPATCHERS.remove_dispatcher_by_name(dispatcher_name)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "remove_dispatcher_by_name"))?;

            remove_dispatcher_by_name_func.call1((dispatcher_name,))?;
            return Ok(());
        };
        dispatchers.retain(|(name, _)| name != dispatcher_name);

        Ok(())
    }
}
