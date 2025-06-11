use pyo3::types::{PyBool, PyString};

use super::prelude::*;

/// Event that triggers when the server tries to set a configstring. You can
/// stop this event and use :func:`shinqlx.set_configstring` to modify it, but a
/// more elegant way to do it is simply returning the new configstring in
/// the handler, and the modified one will go down the plugin chain instead.
#[pyclass(module = "_events", name = "SetConfigstringDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct SetConfigstringDispatcher {}

#[pymethods]
impl SetConfigstringDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "set_configstring";
    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch<'py>(
        slf: &Bound<'py, Self>,
        index: u32,
        value: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(index, value)
    }
}

pub(crate) trait SetConfigstringDispatcherMethods<'py> {
    fn dispatch(&self, index: u32, value: &str) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> SetConfigstringDispatcherMethods<'py> for Bound<'py, SetConfigstringDispatcher> {
    fn dispatch(&self, index: u32, value: &str) -> PyResult<Bound<'py, PyAny>> {
        let mut forwarded_value = value.to_string();
        let mut return_value = PyBool::new(self.py(), true).to_owned().into_any().unbind();

        let plugins = self.as_super().get().plugins.read();
        for handler in (0..5).flat_map(|i| {
            plugins.iter().flat_map(move |(_, handlers)| {
                handlers[i]
                    .iter()
                    .map(|handler| handler.clone_ref(self.py()))
            })
        }) {
            match handler.call1(self.py(), (index, &forwarded_value)) {
                Err(e) => {
                    log_exception(self.py(), &e);
                }
                Ok(res) => match res.extract::<PythonReturnCodes>(self.py()) {
                    Ok(PythonReturnCodes::RET_NONE) => (),
                    Ok(PythonReturnCodes::RET_STOP) => {
                        return Ok(PyBool::new(self.py(), true).to_owned().into_any());
                    }
                    Ok(PythonReturnCodes::RET_STOP_EVENT) => {
                        return_value = PyBool::new(self.py(), false).to_owned().into_any().unbind();
                    }
                    Ok(PythonReturnCodes::RET_STOP_ALL) => {
                        return Ok(PyBool::new(self.py(), false).to_owned().into_any());
                    }
                    _ => match res.extract::<String>(self.py()) {
                        Err(_) => {
                            log_unexpected_return_value(
                                self.py(),
                                SetConfigstringDispatcher::name,
                                res.bind(self.py()),
                                handler.bind(self.py()),
                            );
                        }
                        Ok(str_value) => {
                            forwarded_value.clone_from(&str_value);
                            return_value =
                                PyString::intern(self.py(), &str_value).into_any().unbind();
                        }
                    },
                },
            }
        }

        Ok(return_value.into_bound(self.py()))
    }
}

#[cfg(test)]
mod set_configstring_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{SetConfigstringDispatcher, SetConfigstringDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CS_LEVEL_START_TIME, CVar, CVarBuilder, cvar_t},
            python::{
                PythonReturnCodes,
                commands::CommandPriorities,
                events::EventDispatcherMethods,
                pyshinqlx_setup,
                pyshinqlx_test_support::{
                    python_function_raising_exception, python_function_returning,
                },
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
                .expect("this should not happen");

            let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
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
                    let dispatcher = Bound::new(py, SetConfigstringDispatcher::py_new(py))
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
                    .getattr(intern!(py, "returns_string_hook"))
                    .expect("this should not happen");

                    dispatcher
                        .as_super()
                        .add_hook(
                            "test_plugin",
                            &returns_string_hook,
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(CS_LEVEL_START_TIME, "asdf");
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| bool_value.is_true())
                    }));
                });
            });
    }
}
