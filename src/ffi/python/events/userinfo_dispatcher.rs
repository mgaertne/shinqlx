use core::hint::cold_path;

use pyo3::types::{IntoPyDict, PyBool, PyDict};

use super::prelude::*;
use crate::ffi::python::Player;

/// Event for clients changing their userinfo.
#[pyclass(module = "_events", name = "UserinfoDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UserinfoDispatcher {}

#[pymethods]
impl UserinfoDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "userinfo";
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
        changed: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, changed)
    }
}

pub(crate) trait UserinfoDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        changed: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> UserinfoDispatcherMethods<'py> for Bound<'py, UserinfoDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        changed: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut forwarded_userinfo = changed.to_owned();
        let mut return_value = PyBool::new(self.py(), true).to_owned().into_any().unbind();

        let dbgstr = format!(
            "{}({}, {})",
            UserinfoDispatcher::name,
            player.repr()?,
            changed.repr()?
        );
        dispatcher_debug_log(self.py(), &dbgstr);

        let plugins = self.as_super().get().plugins.read();
        for handler in (0..5).flat_map(|i| {
            plugins.iter().flat_map(move |(_, handlers)| {
                handlers[i]
                    .iter()
                    .map(|handler| handler.clone_ref(self.py()))
            })
        }) {
            match handler.call1(self.py(), (&player, forwarded_userinfo.to_owned())) {
                Err(e) => {
                    cold_path();
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
                    _ => match res.bind(self.py()).downcast::<PyDict>() {
                        Err(_) => {
                            cold_path();
                            log_unexpected_return_value(
                                self.py(),
                                UserinfoDispatcher::name,
                                res.bind(self.py()),
                                handler.bind(self.py()),
                            );
                        }
                        Ok(changed_value) => {
                            forwarded_userinfo =
                                changed_value.to_owned().into_py_dict(self.py())?;
                            return_value = changed_value.to_owned().into_any().unbind();
                        }
                    },
                },
            }
        }

        Ok(return_value.into_bound(self.py()))
    }
}

#[cfg(test)]
mod userinfo_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        prelude::*,
        types::{IntoPyDict, PyBool, PyDict},
    };
    use rstest::rstest;

    use super::{UserinfoDispatcher, UserinfoDispatcherMethods};
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
                Bound::new(py, UserinfoDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                &[("asdf", "qwertz")]
                    .into_py_dict(py)
                    .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
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
    fn dispatch_when_handler_returns_different_dict(_pyshinqlx_setup: ()) {
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
                    let dispatcher = Bound::new(py, UserinfoDispatcher::py_new(py))
                        .expect("this should not happen");

                    let returns_string_hook = python_function_returning(
                        py,
                        &[("qwertz", "asdf")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
                    );
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
                        &[("asdf", "qwertz")]
                            .into_py_dict(py)
                            .expect("this should not happen"),
                    );
                    assert!(
                        result.is_ok_and(|value| value.downcast::<PyDict>().is_ok_and(
                            |dict_value| dict_value
                                .eq([("qwertz", "asdf")]
                                    .into_py_dict(py)
                                    .expect("this should not happen"))
                                .expect("this should not happen")
                        ))
                    );
                });
            });
    }
}
