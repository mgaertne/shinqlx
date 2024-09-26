use super::prelude::*;
use pyo3::types::PyString;

/// Event that triggers whenever a player tries to connect. If the event
/// is not stopped, it will let the player connect as usual. If it is stopped
/// it will either display a generic ban message, or whatever string is returned
/// by the handler.
#[pyclass(module = "_events", name = "PlayerConnectDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct PlayerConnectDispatcher {}

#[pymethods]
impl PlayerConnectDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "player_connect";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    pub(crate) fn handle_return(
        slf: &Bound<'_, Self>,
        handler: PyObject,
        value: PyObject,
    ) -> PyResult<PyObject> {
        if value.bind(slf.py()).is_instance_of::<PyString>() {
            return Ok(value);
        }

        let pyany_event_dispatcher = slf.borrow().into_super().into_py(slf.py());
        let event_dispatcher_instance = pyany_event_dispatcher.bind(slf.py()).downcast()?;
        EventDispatcher::handle_return(event_dispatcher_instance, handler, value)
    }
}

#[cfg(test)]
mod player_connect_dispatcher_tests {
    use super::PlayerConnectDispatcher;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::{commands::CommandPriorities, pyshinqlx_setup};
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use core::borrow::BorrowMut;
    use core::ffi::c_char;

    use mockall::predicate;
    use rstest::rstest;

    use crate::ffi::python::pyshinqlx_test_support::default_test_player;
    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::PyBool;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
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
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
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
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
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
    fn dispatch_when_handler_returns_ret_none(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
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
    fn dispatch_when_handler_returns_ret_stop(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
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
    fn dispatch_when_handler_returns_ret_stop_event(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_all(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<String>()
                .is_ok_and(|str_value| str_value == "return string")));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_unexpected_return_value(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, PlayerConnectDispatcher::py_new(py)).expect("this should not happen");

            let returns_string_hook = PyModule::from_code_bound(
                py,
                r#"
def returns_string_hook(*args, **kwargs):
    return 42
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
                dispatcher.call_method1(py, intern!(py, "dispatch"), (default_test_player(),));
            assert!(result.is_ok_and(|value| value
                .bind(py)
                .extract::<Bound<'_, PyBool>>()
                .is_ok_and(|bool_value| bool_value.is_true())));
        });

        MAIN_ENGINE.store(None);
    }
}
