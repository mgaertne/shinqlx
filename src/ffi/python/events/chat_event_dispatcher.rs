use pyo3::{
    exceptions::PyEnvironmentError,
    types::{PyBool, PyString, PyTuple},
};

use super::{
    super::{COMMANDS, CommandInvokerMethods, Player},
    prelude::*,
};

/// Event that triggers with the "say" command. If the handler cancels it,
/// the message will also be cancelled.
#[pyclass(module = "_events", name = "ChatEventDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ChatEventDispatcher {}

#[pymethods]
impl ChatEventDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "chat";
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
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, msg, channel)
    }
}

pub(crate) trait ChatEventDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> ChatEventDispatcherMethods<'py> for Bound<'py, ChatEventDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        match try_handle_input(self.py(), player, msg, channel) {
            Err(e) => {
                log_exception(self.py(), &e);
            }
            Ok(false) => {
                return Ok(PyBool::new(self.py(), false).to_owned().into_any());
            }
            Ok(true) => (),
        };

        let args_tuple = PyTuple::new(
            self.py(),
            [
                player.as_any(),
                PyString::new(self.py(), msg).as_any(),
                channel,
            ],
        )?;

        Ok(self.as_super().dispatch(&args_tuple))
    }
}

fn try_handle_input(
    py: Python<'_>,
    player: &Bound<'_, Player>,
    cmd: &str,
    channel: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    COMMANDS.load().as_ref().map_or(
        Err(PyEnvironmentError::new_err(
            "could not get access to COMMANDS",
        )),
        |commands| {
            commands
                .bind(py)
                .handle_input(player, cmd, channel.as_any())
        },
    )
}

#[cfg(test)]
mod chat_event_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        prelude::*,
        types::{PyBool, PyString},
    };
    use rstest::rstest;

    use super::{ChatEventDispatcher, ChatEventDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                COMMANDS, PythonReturnCodes,
                channels::TeamChatChannel,
                commands::{Command, CommandInvoker, CommandInvokerMethods, CommandPriorities},
                events::EventDispatcherMethods,
                pyshinqlx_setup,
                pyshinqlx_test_support::{
                    default_test_player, python_function_raising_exception,
                    python_function_returning, test_plugin,
                },
            },
        },
        prelude::*,
    };

    fn default_channel(py: Python<'_>) -> Bound<'_, PyAny> {
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
        .expect("this should not happen");
        channel.into_any()
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        COMMANDS.store(None);

        Python::with_gil(|py| {
            let dispatcher =
                Bound::new(py, ChatEventDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                "asdf",
                &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let throws_exception_hook = python_function_raising_exception(py);
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_none_hook =
                        python_function_returning(py, &py.None().into_bound(py));
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_none_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_NONE as i32));
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_stop_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP as i32));
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_stop_event_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP_EVENT as i32));
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_stop_all_hook =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP_ALL as i32));
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
                        &default_channel(py),
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
                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_string_hook = python_function_returning(py, &"return string");
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
                        &default_channel(py),
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
                    let cmd_handler =
                        python_function_returning(py, &(PythonReturnCodes::RET_STOP as i32));
                    let command = Command::py_new(
                        &plugin,
                        PyString::intern(py, "asdf").as_any(),
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

                    let dispatcher = Bound::new(py, ChatEventDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_none_hook =
                        python_function_returning(py, &py.None().into_bound(py));
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
                        &default_channel(py),
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|value| !value.is_true())
                    }),);
                });
            });
    }
}
