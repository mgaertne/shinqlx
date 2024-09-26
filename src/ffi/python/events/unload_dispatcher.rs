use super::prelude::*;

/// Event that triggers whenever a plugin is unloaded. Cannot be cancelled.
#[pyclass(module = "_events", name = "UnloadDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UnloadDispatcher {}

#[pymethods]
impl UnloadDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "unload";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}

#[cfg(test)]
mod unload_dispatcher_tests {
    use super::UnloadDispatcher;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::{commands::CommandPriorities, pyshinqlx_setup};
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use core::ffi::c_char;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyTuple};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
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
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(&mut raw_cvar as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_none(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_none(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_event(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_all(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, UnloadDispatcher::py_new(py)).expect("this should not happen");

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

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });
    }
}
