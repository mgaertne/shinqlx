use pyo3::types::{PyBool, PyTuple};

use super::prelude::*;
use crate::ffi::python::Player;

/// Event that goes off whenever someone tries to vote either yes or no.
#[pyclass(module = "_events", name = "VoteDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteDispatcher {}

#[pymethods]
impl VoteDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote";
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
        yes: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, yes)
    }
}

pub(crate) trait VoteDispatcherMethods<'py> {
    fn dispatch(&self, player: &Bound<'py, Player>, yes: bool) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> VoteDispatcherMethods<'py> for Bound<'py, VoteDispatcher> {
    fn dispatch(&self, player: &Bound<'py, Player>, yes: bool) -> PyResult<Bound<'py, PyAny>> {
        let args_tuple = PyTuple::new(
            self.py(),
            [player.as_any(), PyBool::new(self.py(), yes).as_any()],
        )?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod vote_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{VoteDispatcher, VoteDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
                pyshinqlx_test_support::default_test_player,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
                    let dispatcher =
                        Bound::new(py, VoteDispatcher::py_new(py)).expect("this should not happen");

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
                        true,
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
