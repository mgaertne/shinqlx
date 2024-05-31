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
    PyTraverseError, PyVisit,
};

use itertools::Itertools;
use pyo3::types::IntoPyDict;

fn try_dispatcher_debug_log(py: Python<'_>, debug_str: &str) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let debug_level = py
            .import_bound(intern!(py, "logging"))
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
                Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict_bound(py)),
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
    result: &PyObject,
    handler: &PyObject,
) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let warning_level = py
            .import_bound(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "WARNING")))?;
        let handler_name = handler.getattr(py, intern!(py, "__name__"))?;

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
                Some(&[(intern!(py, "func"), intern!(py, "dispatch"))].into_py_dict_bound(py)),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

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
        NO_DEBUG.into_iter().collect_tuple().unwrap_or_default()
    }

    #[new]
    pub(crate) fn py_new(_py: Python<'_>) -> Self {
        Self::default()
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let events = &(*self.plugins.read());
        events
            .iter()
            .map(|(_, plugins)| {
                plugins
                    .iter()
                    .map(|prio_plugins| {
                        prio_plugins
                            .iter()
                            .map(|plugin| visit.call(plugin))
                            .collect::<Result<Vec<_>, PyTraverseError>>()
                    })
                    .collect::<Result<Vec<_>, PyTraverseError>>()
            })
            .collect::<Result<Vec<_>, PyTraverseError>>()
            .map(|_| ())
    }

    fn __clear__(&mut self) {
        let events = &mut (*self.plugins.write());
        for (_, plugins) in events {
            for prio_plugins in plugins {
                prio_plugins.clear();
            }
        }
    }

    #[getter(plugins)]
    fn get_plugins(slf: Bound<'_, Self>) -> Bound<'_, PyDict> {
        let Ok(event_dispatcher) = slf.try_borrow() else {
            return PyDict::new_bound(slf.py());
        };
        let plugins = event_dispatcher.plugins.read();
        plugins.clone().into_py_dict_bound(slf.py())
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
    pub(crate) fn dispatch(slf: &Bound<'_, Self>, args: Bound<'_, PyTuple>) -> PyObject {
        let Ok(event_dispatcher) = slf.try_borrow() else {
            return slf.py().None();
        };
        let Ok(py_dispatcher_name) = slf.get_type().getattr(intern!(slf.py(), "name")) else {
            return slf.py().None();
        };
        let Ok(dispatcher_name) = py_dispatcher_name.extract::<String>() else {
            return slf.py().None();
        };
        if !NO_DEBUG.contains(&dispatcher_name.as_str()) {
            let dbgstr = format!("{}{}", dispatcher_name, &args);
            dispatcher_debug_log(slf.py(), &dbgstr);
        }

        let mut return_value = true.into_py(slf.py());

        let plugins = event_dispatcher.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.clone() {
                for handler in &handlers[i] {
                    let handler_args = PyTuple::new_bound(slf.py(), &args);
                    match handler.call1(slf.py(), handler_args) {
                        Err(e) => {
                            log_exception(slf.py(), &e);
                            continue;
                        }
                        Ok(res) => {
                            let res_i32 = res.extract::<PythonReturnCodes>(slf.py());
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
                                return true.into_py(slf.py());
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                return_value = false.into_py(slf.py());
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return false.into_py(slf.py());
                            }

                            match Self::handle_return(slf, handler.into_py(slf.py()), res) {
                                Err(e) => {
                                    log_exception(slf.py(), &e);
                                    continue;
                                }
                                Ok(return_handler) => {
                                    if !return_handler.is_none(slf.py()) {
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
        slf: &Bound<'_, Self>,
        handler: PyObject,
        value: PyObject,
    ) -> PyResult<PyObject> {
        let dispatcher_name = slf
            .get_type()
            .getattr(intern!(slf.py(), "name"))?
            .extract::<String>()?;
        log_unexpected_return_value(slf.py(), &dispatcher_name, &value, &handler);

        Ok(slf.py().None())
    }

    /// Hook the event, making the handler get called with relevant arguments
    /// whenever the event is takes place.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "(plugin, handler, priority=PRI_NORMAL)")]
    pub(crate) fn add_hook(
        slf: &Bound<'_, Self>,
        plugin: &str,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        if !(0i32..5i32).contains(&priority) {
            let error_description = format!("'{}' is an invalid priority level.", priority);
            return Err(PyValueError::new_err(error_description));
        }

        let event_dispatcher = slf
            .try_borrow()
            .map_err(|_| PyValueError::new_err("could not borrow event_dispatcher"))?;
        let dispatcher_name = slf
            .get_type()
            .getattr(intern!(slf.py(), "name"))?
            .extract::<String>()
            .map_err(|_| {
                PyValueError::new_err("Cannot add a hook from an event dispatcher with no name.")
            })?;

        let need_zmq_stats_enabled = slf
            .get_type()
            .getattr(intern!(slf.py(), "need_zmq_stats_enabled"))?
            .extract::<bool>()
            .map_err(|_| {
                PyValueError::new_err("Cannot add a hook from an event dispatcher with no need_zmq_stats_enabled flag.")
            })?;

        let zmq_enabled_cvar = pyshinqlx_get_cvar(slf.py(), "zmq_stats_enable")?;
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
                let add_hook_func = PyModule::from_code_bound(
                    slf.py(),
                    r#"
import shinqlx


@shinqlx.next_frame
def add_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].add_hook(plugin, handler, priority)
        "#,
                    "",
                    "",
                )?
                .getattr(intern!(slf.py(), "add_hook"))?;

                add_hook_func.call1((&dispatcher_name, plugin, handler, priority))?;
            }
            Some(mut plugins) => {
                let Some(plugin_hooks) = plugins
                    .iter_mut()
                    .find(|(added_plugin, _)| added_plugin == plugin)
                else {
                    let mut new_hooks =
                        (plugin.to_string(), [vec![], vec![], vec![], vec![], vec![]]);
                    new_hooks.1[priority as usize].push(handler);
                    plugins.push(new_hooks);
                    return Ok(());
                };

                if plugin_hooks.1[priority as usize]
                    .iter()
                    .any(|registered_command| {
                        registered_command
                            .bind(slf.py())
                            .eq(handler.bind(slf.py()))
                            .unwrap_or(false)
                    })
                {
                    return Err(PyValueError::new_err(
                        "The event has already been hooked with the same handler and priority.",
                    ));
                }

                plugin_hooks.1[priority as usize].push(handler);
            }
        }
        Ok(())
    }

    /// Removes a previously hooked event.
    #[pyo3(signature = (plugin, handler, priority=CommandPriorities::PRI_NORMAL as i32), text_signature = "(plugin, handler, priority=PRI_NORMAL)")]
    fn remove_hook(
        slf: &Bound<'_, Self>,
        plugin: &str,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let event_dispatcher = slf
            .try_borrow()
            .map_err(|_| PyValueError::new_err("could not borrow event_dispatcher"))?;
        let dispatcher_name = slf
            .get_type()
            .getattr(intern!(slf.py(), "name"))?
            .extract::<String>()
            .map_err(|_| {
                PyValueError::new_err("Cannot remove a hook from an event dispatcher with no name.")
            })?;
        match event_dispatcher.plugins.try_write() {
            None => {
                let remove_hook_func = PyModule::from_code_bound(
                    slf.py(),
                    r#"
import shinqlx


@shinqlx.next_frame
def remove_hook(event, plugin, handler, priority):
    shinqlx.EVENT_DISPATCHERS[event].remove_hook(plugin, handler, priority)
        "#,
                    "",
                    "",
                )?
                .getattr(intern!(slf.py(), "remove_hook"))?;
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

                if !plugin_hooks.1[priority as usize].iter().any(|item| {
                    item.bind(slf.py())
                        .eq(handler.bind(slf.py()))
                        .unwrap_or(true)
                }) {
                    return Err(PyValueError::new_err(
                        "The event has not been hooked with the handler provided",
                    ));
                }

                plugin_hooks.1[priority as usize].retain(|item| {
                    item.bind(slf.py())
                        .ne(handler.bind(slf.py()))
                        .unwrap_or(true)
                });
            }
        }
        Ok(())
    }
}

/// Holds all the event dispatchers and provides a way to access the dispatcher
/// instances by accessing it like a dictionary using the event name as a key.
/// Only one dispatcher can be used per event.
#[pyclass(name = "EventDispatcherManager", module = "_events", mapping)]
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

    fn __clear__(&mut self) {
        self.dispatchers.write().clear();
    }

    #[getter(_dispatchers)]
    fn get_dispatchers<'py>(&'py self, py: Python<'py>) -> Bound<'py, PyDict> {
        let dispatchers = self.dispatchers.read();
        dispatchers.clone().into_py_dict_bound(py)
    }

    pub(crate) fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        self.dispatchers
            .read()
            .iter()
            .find(|(event_name, _)| key == event_name)
            .map_or_else(
                || {
                    let key_error = format!("'{}'", key);
                    Err(PyKeyError::new_err(key_error))
                },
                |(_, dispatcher)| Ok(dispatcher.clone_ref(py)),
            )
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> bool {
        py.allow_threads(|| {
            self.dispatchers
                .read()
                .iter()
                .any(|(event_name, _)| key == event_name)
        })
    }

    pub(crate) fn add_dispatcher(
        &self,
        py: Python<'_>,
        dispatcher: Bound<'_, PyType>,
    ) -> PyResult<()> {
        if !dispatcher
            .is_subclass_of::<EventDispatcher>()
            .unwrap_or(false)
        {
            return Err(PyValueError::new_err(
                "Cannot add an event dispatcher not based on EventDispatcher.",
            ));
        }

        let dispatcher_name_str = dispatcher
            .getattr(intern!(py, "name"))
            .and_then(|dispatcher_name_attr| dispatcher_name_attr.extract::<String>())
            .map_err(|_| PyValueError::new_err("Cannot add an event dispatcher with no name."))?;
        if self.__contains__(py, &dispatcher_name_str) {
            return Err(PyValueError::new_err("Event name already taken."));
        }

        self.dispatchers
            .write()
            .push((dispatcher_name_str, dispatcher.call0()?.unbind()));

        Ok(())
    }

    fn remove_dispatcher(&self, py: Python<'_>, dispatcher: PyObject) -> PyResult<()> {
        let dispatcher_name_str = dispatcher
            .getattr(py, intern!(py, "name"))
            .and_then(|dispatcher_name_attr| dispatcher_name_attr.extract::<String>(py))
            .map_err(|_| {
                PyValueError::new_err("Cannot remove an event dispatcher with no name.")
            })?;

        self.remove_dispatcher_by_name(py, &dispatcher_name_str)
    }

    fn remove_dispatcher_by_name(&self, py: Python<'_>, dispatcher_name: &str) -> PyResult<()> {
        if !self.__contains__(py, dispatcher_name) {
            return Err(PyValueError::new_err("Event name not found."));
        }

        match self.dispatchers.try_write() {
            None => {
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
        EventDispatcherManager, GameCountdownDispatcher, GameEndDispatcher, GameStartDispatcher,
    };
    use pyo3::exceptions::PyValueError;

    use crate::ffi::python::plugin::Plugin;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    use rstest::*;

    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn event_dispatcher_manager_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            let result = py
                .import_bound("gc")
                .and_then(|gc| gc.call_method0("collect"));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_when_no_dispatchers_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);

            let result = event_dispatchers.get_dispatchers(py);
            assert!(result.is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_dispatchers_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            let result = event_dispatchers.get_dispatchers(py);
            assert!(result
                .get_item("game_countdown")
                .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                    dispatcher.is_instance_of::<GameCountdownDispatcher>()
                })));
            assert!(result
                .get_item("game_start")
                .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                    dispatcher.is_instance_of::<GameStartDispatcher>()
                })));
            assert!(result
                .get_item("game_end")
                .is_ok_and(|opt_dispatcher| opt_dispatcher.is_some_and(|dispatcher| {
                    dispatcher.is_instance_of::<GameEndDispatcher>()
                })));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_with_added_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameStartDispatcher>())
                .expect("could not add game_start dispatcher");
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameEndDispatcher>())
                .expect("could not add game_end dispatcher");

            assert!(event_dispatchers.__contains__(py, "game_countdown"));
            assert!(event_dispatchers.__contains__(py, "game_start"));
            assert!(event_dispatchers.__contains__(py, "game_end"));
            assert!(!event_dispatchers.__contains__(py, "map"));
            assert!(!event_dispatchers.__contains__(py, "damage"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_with_type_not_being_subclass_of_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);

            let result = event_dispatchers.add_dispatcher(py, py.get_type_bound::<Plugin>());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");

            let result = event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_with_type_not_being_subclass_of_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);

            let result = event_dispatchers
                .remove_dispatcher(py, py.get_type_bound::<Plugin>().into_any().unbind());

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_that_is_already_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);
            event_dispatchers
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");

            let result = event_dispatchers.remove_dispatcher(
                py,
                py.get_type_bound::<GameCountdownDispatcher>()
                    .into_any()
                    .unbind(),
            );

            assert!(result.is_ok());
            assert!(!event_dispatchers.__contains__(py, "game_countdown"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_dispatcher_that_is_not_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let event_dispatchers = EventDispatcherManager::py_new(py);

            let result = event_dispatchers.remove_dispatcher(
                py,
                py.get_type_bound::<GameCountdownDispatcher>()
                    .into_any()
                    .unbind(),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }
}
