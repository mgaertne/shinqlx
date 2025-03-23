use super::super::channels::ClientCommandChannel;
use super::super::{COMMANDS, CommandInvokerMethods, Player};
use super::prelude::*;
use std::ops::Deref;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::types::{PyBool, PyString};

/// Event that triggers with any client command. This overlaps with
/// other events, such as "chat".
#[pyclass(module = "_events", name = "ClientCommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ClientCommandDispatcher {}

#[pymethods]
impl ClientCommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "client_command";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch<'py>(
        slf: &Bound<'py, Self>,
        player: &Bound<'py, Player>,
        cmd: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, cmd)
    }
}

pub(crate) trait ClientCommandDispatcherMethods<'py> {
    fn dispatch(&self, player: &Bound<'py, Player>, cmd: &str) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> ClientCommandDispatcherMethods<'py> for Bound<'py, ClientCommandDispatcher> {
    fn dispatch(&self, player: &Bound<'py, Player>, cmd: &str) -> PyResult<Bound<'py, PyAny>> {
        let mut forwarded_cmd = cmd.to_string();
        let mut return_value = PyBool::new(self.py(), true).to_owned().into_any().unbind();

        let super_class = self.borrow().into_super();
        let dbgstr = format!(
            "{}({}, {})",
            ClientCommandDispatcher::name,
            player.repr()?,
            cmd
        );
        dispatcher_debug_log(self.py(), &dbgstr);
        let plugins = super_class.plugins.read();

        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(self.py(), (player, &forwarded_cmd)) {
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
                                return Ok(PyBool::new(self.py(), true).to_owned().into_any());
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
                                return Ok(PyBool::new(self.py(), false).to_owned().into_any());
                            }

                            let Ok(str_value) = res.extract::<String>(self.py()) else {
                                log_unexpected_return_value(
                                    self.py(),
                                    ClientCommandDispatcher::name,
                                    res.bind(self.py()),
                                    handler.bind(self.py()),
                                );
                                continue;
                            };
                            forwarded_cmd.clone_from(&str_value);
                            return_value = PyString::new(self.py(), &str_value).into_any().unbind();
                        }
                    }
                }
            }
        }

        if return_value
            .bind(self.py())
            .downcast::<PyBool>()
            .is_ok_and(|value| !value.is_true())
        {
            return Ok(PyBool::new(self.py(), false).to_owned().into_any());
        }

        match try_handle_input(self.py(), player, cmd) {
            Err(e) => {
                log_exception(self.py(), &e);
            }
            Ok(handle_input_return) => {
                if !handle_input_return {
                    return Ok(PyBool::new(self.py(), false).to_owned().into_any());
                }
            }
        };

        Ok(return_value.bind(self.py()).to_owned())
    }
}

fn try_handle_input(py: Python<'_>, player: &Bound<'_, Player>, cmd: &str) -> PyResult<bool> {
    let client_command_channel =
        Bound::new(py, ClientCommandChannel::py_new(player.borrow().deref()))?;
    COMMANDS.load().as_ref().map_or(
        Err(PyEnvironmentError::new_err(
            "could not get access to COMMANDS",
        )),
        |commands| {
            commands
                .bind(py)
                .handle_input(player, cmd, client_command_channel.as_any())
        },
    )
}

#[cfg(test)]
mod client_command_dispatcher_tests {
    use super::{ClientCommandDispatcher, ClientCommandDispatcherMethods};

    use crate::ffi::c::prelude::{CVar, CVarBuilder, cvar_t};
    use crate::ffi::python::{
        COMMANDS,
        commands::{Command, CommandInvoker, CommandInvokerMethods, CommandPriorities},
        events::EventDispatcherMethods,
        pyshinqlx_setup,
        pyshinqlx_test_support::{default_test_player, test_plugin},
    };
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::rstest;

    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyString};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        COMMANDS.store(None);
        Python::with_gil(|py| {
            let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                .expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                "asdf",
            );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &throws_exception_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_none_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_none_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_stop_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_stop_event_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_stop_all_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_string_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "return string")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_value_with_no_string(_pyshinqlx_setup: ()) {
        COMMANDS.store(None);
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
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_string_hook = PyModule::from_code(
                        py,
                        cr#"
class NonStringObject:
    def __str__(self):
        raise NotImplemented("__str__ not implemented")

def returns_string_hook(*args, **kwargs):
    return NonStringObject()
            "#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen")
                    .getattr("returns_string_hook")
                    .expect("this should not happen");

                    dispatcher
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_string_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
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
    fn dispatch_when_command_handler_returns_false(_pyshinqlx_setup: ()) {
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
            .with_find_cvar(|cmd| cmd != "zmq_stats_enable", |_| None, 0..)
            .run(|| {
                Python::with_gil(|py| {
                    let plugin = test_plugin(py).call0().expect("this should not happen");
                    let cmd_handler = PyModule::from_code(
                        py,
                        cr#"
import shinqlx

def handler(*args, **kwargs):
    return shinqlx.RET_STOP
            "#,
                        c"",
                        c"",
                    )
                    .expect("could not get module from code")
                    .getattr("handler")
                    .expect("could not get handler");
                    let command = Command::py_new(
                        &plugin,
                        PyString::new(py, "asdf").as_any(),
                        &cmd_handler,
                        0,
                        py.None().bind(py),
                        py.None().bind(py),
                        false,
                        0,
                        false,
                        "",
                    )
                    .expect("could not create command");
                    let py_command = Bound::new(py, command).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("could not add command to command invoker");
                    COMMANDS.store(Some(command_invoker.unbind().into()));

                    let dispatcher = Bound::new(py, ClientCommandDispatcher::py_new(py))
                        .expect("this should not happen");

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
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_none_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "asdf",
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|value| !value.is_true())
                    }));
                });
            });
    }
}
