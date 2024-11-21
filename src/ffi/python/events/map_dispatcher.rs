use super::prelude::*;

/// Event that goes off when a map is loaded, even if the same map is loaded again.
#[pyclass(module = "_events", name = "MapDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct MapDispatcher {}

#[pymethods]
impl MapDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "map";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}

#[cfg(test)]
mod map_dispatcher_tests {
    use super::MapDispatcher;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::{commands::CommandPriorities, pyshinqlx_setup};
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyTuple};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                throws_exception_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_none_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_none_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_event_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_stop_all_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
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
                        Py::new(py, MapDispatcher::py_new(py)).expect("this should not happen");

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
                            py,
                            intern!(py, "add_hook"),
                            (
                                "test_plugin",
                                returns_string_hook.unbind(),
                                CommandPriorities::PRI_NORMAL as i32,
                            ),
                        )
                        .expect("this should not happen");

                    let result =
                        dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty(py));
                    assert!(result.is_ok_and(|value| value
                        .bind(py)
                        .extract::<Bound<'_, PyBool>>()
                        .is_ok_and(|bool_value| bool_value.is_true())));
                });
            });
    }
}
