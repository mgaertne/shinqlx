use pyo3::types::{PyString, PyTuple};

use super::prelude::*;
use crate::ffi::python::{Command, Player};

/// Event that goes off when a command is executed. This can be used
/// to for instance keep a log of all the commands admins have used.
#[pyclass(module = "_events", name = "CommandDispatcher", extends = EventDispatcher, frozen)]
#[derive(Clone)]
pub(crate) struct CommandDispatcher {}

#[pymethods]
impl CommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "command";
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
        command: &Bound<'py, Command>,
        args: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, command, args)
    }
}

pub(crate) trait CommandDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        command: &Bound<'py, Command>,
        args: &str,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> CommandDispatcherMethods<'py> for Bound<'py, CommandDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        command: &Bound<'py, Command>,
        args: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let args_tuple = PyTuple::new(
            self.py(),
            [
                player.as_any(),
                command.as_any(),
                PyString::new(self.py(), args).as_any(),
            ],
        )?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod command_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{CommandDispatcher, CommandDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                PythonReturnCodes,
                commands::CommandPriorities,
                events::EventDispatcherMethods,
                pyshinqlx_setup,
                pyshinqlx_test_support::{
                    default_command, default_test_player, python_function_raising_exception,
                    python_function_returning,
                },
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Bound::new(py, CommandDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                &Bound::new(py, default_command(py)).expect("this should not happen"),
                "asdf",
            );
            assert!(result.is_ok_and(|value| {
                value
                    .downcast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            }),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_exception(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
                        "asdf",
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }),);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_none(_pyshinqlx_setup: ()) {
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, CommandDispatcher::py_new(py))
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
                        &Bound::new(py, default_command(py)).expect("this should not happen"),
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
}
