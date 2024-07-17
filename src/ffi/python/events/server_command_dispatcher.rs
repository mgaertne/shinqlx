use super::prelude::*;

/// Event that triggers with any server command sent by the server,
/// including :func:`shinqlx.send_server_command`. Can be cancelled.
#[pyclass(module = "_events", name = "ServerCommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ServerCommandDispatcher {}

#[pymethods]
impl ServerCommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "server_command";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, player: PyObject, cmd: &str) -> PyObject {
        let mut forwarded_cmd = cmd.to_string();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&player, &forwarded_cmd)) {
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

        return_value
    }
}

#[cfg(test)]
mod server_command_dispatcher_tests {
    use super::ServerCommandDispatcher;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::pyshinqlx_test_support::default_test_player;
    use crate::ffi::python::{commands::CommandPriorities, pyshinqlx_setup};
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use alloc::ffi::CString;
    use core::ffi::c_char;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.call_method1(
                py,
                intern!(py, "dispatch"),
                (default_test_player(), "asdf"),
            );
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_exception(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_none(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_none(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_event(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| !value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_ret_stop_all(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| !value.bind(py).is_truthy().expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_string(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_value_with_no_string(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("1").expect("this should not happen");
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
                Py::new(py, ServerCommandDispatcher::py_new(py)).expect("this should not happen");

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
            assert!(result
                .is_ok_and(|value| value.bind(py).is_truthy().expect("this should not happen")));
        });
    }
}
