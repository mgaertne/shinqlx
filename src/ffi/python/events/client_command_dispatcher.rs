use super::super::{Player, COMMANDS};
use super::prelude::*;
use crate::ffi::python::channels::ClientCommandChannel;
use pyo3::exceptions::PyEnvironmentError;
use pyo3::types::PyBool;

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

    fn dispatch(slf: &Bound<'_, Self>, player: Player, cmd: &str) -> PyObject {
        let mut forwarded_cmd = cmd.to_string();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        let player_str = &player.name;
        let dbgstr = format!("{}({}, {})", Self::name, player_str, cmd);
        dispatcher_debug_log(slf.py(), &dbgstr);
        let plugins = super_class.plugins.read();

        let py_player = player.clone().into_py(slf.py());
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&py_player, &forwarded_cmd)) {
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

                            let Ok(str_value) = res.extract::<String>(slf.py()) else {
                                log_unexpected_return_value(slf.py(), Self::name, &res, handler);
                                continue;
                            };
                            forwarded_cmd.clone_from(&str_value);
                            return_value = str_value.clone().into_py(slf.py());
                        }
                    }
                }
            }
        }

        if return_value
            .extract::<Bound<'_, PyBool>>(slf.py())
            .is_ok_and(|value| !value.is_true())
        {
            return false.into_py(slf.py());
        }

        match try_handle_input(slf.py(), &player, cmd) {
            Err(e) => {
                log_exception(slf.py(), &e);
            }
            Ok(handle_input_return) => {
                if !handle_input_return {
                    return false.into_py(slf.py());
                }
            }
        };

        return_value
    }
}

fn try_handle_input(py: Python<'_>, player: &Player, cmd: &str) -> PyResult<bool> {
    let client_command_channel = Py::new(py, ClientCommandChannel::py_new(player))?;
    COMMANDS.load().as_ref().map_or(
        Err(PyEnvironmentError::new_err(
            "could not get access to COMMANDS",
        )),
        |commands| {
            commands
                .borrow(py)
                .handle_input(py, player, cmd, client_command_channel.into_py(py))
        },
    )
}

#[cfg(test)]
mod client_command_dispatcher_tests {
    use super::ClientCommandDispatcher;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::pyshinqlx_test_support::{default_test_player, test_plugin};
    use crate::ffi::python::{
        commands::{Command, CommandInvoker, CommandPriorities},
        pyshinqlx_setup, COMMANDS,
    };
    use crate::prelude::{serial, with_mocked_engine};

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::PyBool;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        COMMANDS.store(None);
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, ClientCommandDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.call_method1(
                py,
                intern!(py, "dispatch"),
                (default_test_player(), "asdf"),
            );
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let throws_exception_hook = PyModule::from_code_bound(
                    py,
                    r#"
def throws_exception_hook(*args, **kwargs):
    raise ValueError("asdf")
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("throws_exception_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            throws_exception_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_none_hook = PyModule::from_code_bound(
                    py,
                    r#"
def returns_none_hook(*args, **kwargs):
    return None
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_none_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_none_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_none_hook = PyModule::from_code_bound(
                    py,
                    r#"
import shinqlx

def returns_none_hook(*args, **kwargs):
    return shinqlx.RET_NONE
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_none_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_none_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_stop_hook = PyModule::from_code_bound(
                    py,
                    r#"
import shinqlx

def returns_stop_hook(*args, **kwargs):
    return shinqlx.RET_STOP
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_stop_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_stop_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_stop_event_hook = PyModule::from_code_bound(
                    py,
                    r#"
import shinqlx

def returns_stop_event_hook(*args, **kwargs):
    return shinqlx.RET_STOP_EVENT
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_stop_event_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_stop_event_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| !bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_stop_all_hook = PyModule::from_code_bound(
                    py,
                    r#"
import shinqlx

def returns_stop_all_hook(*args, **kwargs):
    return shinqlx.RET_STOP_ALL
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_stop_all_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_stop_all_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| !bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_string_hook = PyModule::from_code_bound(
                    py,
                    r#"
def returns_string_hook(*args, **kwargs):
    return "return string"
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_string_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_string_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<String>()
                    .is_ok_and(|str_value| str_value == "return string")));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        })
        .run(|| {
            Python::with_gil(|py| {
                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_string_hook = PyModule::from_code_bound(
                    py,
                    r#"
class NonStringObject:
    def __str__(self):
        raise NotImplemented("__str__ not implemented")

def returns_string_hook(*args, **kwargs):
    return NonStringObject()
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_string_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_string_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|bool_value| bool_value.is_true())));
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
        with_mocked_engine(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .with(predicate::eq("zmq_stats_enable"))
                .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
            mock_engine.expect_find_cvar().returning(|_| None);
        })
        .run(|| {
            Python::with_gil(|py| {
                let plugin = test_plugin(py);
                let cmd_handler = PyModule::from_code_bound(
                    py,
                    r#"
import shinqlx

def handler(*args, **kwargs):
    return shinqlx.RET_STOP
            "#,
                    "",
                    "",
                )
                .expect("could not get module from code")
                .getattr("handler")
                .expect("could not get handler");
                let command_invoker = CommandInvoker::py_new();
                let command = Command::py_new(
                    py,
                    plugin.unbind(),
                    "asdf".into_py(py),
                    cmd_handler.unbind(),
                    0,
                    py.None(),
                    py.None(),
                    false,
                    0,
                    false,
                    "",
                )
                .expect("could not create command");
                let py_command = Py::new(py, command).expect("this should not happen");
                command_invoker
                    .add_command(
                        py,
                        py_command.into_bound(py),
                        CommandPriorities::PRI_NORMAL as usize,
                    )
                    .expect("could not add command to command invoker");
                COMMANDS.store(Some(
                    Py::new(py, command_invoker)
                        .expect("could not create command invoker in python")
                        .into(),
                ));

                let dispatcher = Py::new(py, ClientCommandDispatcher::py_new(py))
                    .expect("this should not happen");

                let returns_none_hook = PyModule::from_code_bound(
                    py,
                    r#"
def returns_none_hook(*args, **kwargs):
    return None
            "#,
                    "",
                    "",
                )
                .expect("this should not happen")
                .getattr("returns_none_hook")
                .expect("this should not happen");

                dispatcher
                    .call_method1(
                        py,
                        intern!(py, "add_hook"),
                        (
                            "test_plugin",
                            returns_none_hook.unbind(),
                            CommandPriorities::PRI_NORMAL as i32,
                        ),
                    )
                    .expect("this should not happen");

                let result = dispatcher.call_method1(
                    py,
                    intern!(py, "dispatch"),
                    (default_test_player(), "asdf"),
                );
                assert!(result.is_ok_and(|value| value
                    .bind(py)
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|value| !value.is_true())));
            });
        });
    }
}
