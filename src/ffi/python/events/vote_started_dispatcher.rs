use pyo3::types::{PyString, PyTuple};

use super::prelude::*;

/// Event that goes off whenever a vote starts. A vote started with Plugin.callvote()
/// will have the caller set to None.
#[pyclass(module = "_events", name = "VoteStartedDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteStartedDispatcher {
    player: parking_lot::RwLock<PyObject>,
}

#[pymethods]
impl VoteStartedDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote_started";
    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(py: Python<'_>) -> (Self, EventDispatcher) {
        (
            Self {
                player: py.None().into(),
            },
            EventDispatcher::default(),
        )
    }

    fn dispatch<'py>(
        slf: &Bound<'py, Self>,
        vote: &str,
        args: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(vote, args)
    }

    fn caller(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) {
        slf.caller(player)
    }
}

pub(crate) trait VoteStartedDispatcherMethods<'py> {
    fn dispatch(&self, vote: &str, args: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>>;
    fn caller(&self, player: &Bound<'py, PyAny>);
}

impl<'py> VoteStartedDispatcherMethods<'py> for Bound<'py, VoteStartedDispatcher> {
    fn dispatch(&self, vote: &str, args: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let player = &self
            .get()
            .player
            .try_read()
            .unwrap()
            .clone_ref(self.py())
            .into_bound(self.py())
            .into_any();

        let args_tuple = PyTuple::new(
            self.py(),
            [player, PyString::intern(self.py(), vote).as_any(), args],
        )?;

        Ok(self.as_super().dispatch(&args_tuple))
    }

    fn caller(&self, player: &Bound<'py, PyAny>) {
        *self.get().player.write() = player.as_unbound().clone_ref(self.py());
    }
}

#[cfg(test)]
mod vote_started_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        prelude::*,
        types::{PyBool, PyString},
    };
    use rstest::rstest;

    use super::{VoteStartedDispatcher, VoteStartedDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                PythonReturnCodes,
                commands::CommandPriorities,
                events::EventDispatcherMethods,
                pyshinqlx_setup,
                pyshinqlx_test_support::{
                    default_test_player, python_function_raising_exception,
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
                Bound::new(py, VoteStartedDispatcher::py_new(py)).expect("this should not happen");
            dispatcher.caller(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
            );

            let result = dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

                    let throws_exception_hook = python_function_raising_exception(py);
                    dispatcher
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &throws_exception_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

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

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

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

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

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

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

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

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

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

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
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
                    let dispatcher = Bound::new(py, VoteStartedDispatcher::py_new(py))
                        .expect("this should not happen");
                    dispatcher.caller(
                        Bound::new(py, default_test_player())
                            .expect("this should not happen")
                            .as_any(),
                    );

                    let returns_string_hook = python_function_returning(py, &"return string");
                    dispatcher
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_string_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.dispatch("map", PyString::intern(py, "thunderstruck").as_any());
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }
}
