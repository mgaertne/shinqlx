use core::hint::cold_path;
use std::sync::LazyLock;

use pyo3::{
    exceptions::PyEnvironmentError,
    types::{PyBool, PyString, PyTuple},
};
use regex::Regex;

use super::prelude::*;
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES},
    quake_live_engine::GetConfigstring,
};

static RE_VOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^(?P<cmd>[^ ]+)(?: "?(?P<args>.*?)"?)?$"#).unwrap());

/// Event that goes off whenever a vote either passes or fails.
#[pyclass(module = "_events", name = "VoteEndedDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteEndedDispatcher {}

#[pymethods]
impl VoteEndedDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote_ended";
    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, passed: bool) -> PyResult<()> {
        slf.dispatch(passed)
    }
}

pub(crate) trait VoteEndedDispatcherMethods<'py> {
    fn dispatch(&self, passed: bool) -> PyResult<()>;
}

impl<'py> VoteEndedDispatcherMethods<'py> for Bound<'py, VoteEndedDispatcher> {
    fn dispatch(&self, passed: bool) -> PyResult<()> {
        let configstring = MAIN_ENGINE.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ))
            },
            |main_engine| Ok(main_engine.get_configstring(CS_VOTE_STRING as u16)),
        )?;

        if configstring.is_empty() {
            cold_path();
            dispatcher_debug_log(
                self.py(),
                "vote_ended went off without configstring CS_VOTE_STRING.",
            );
            return Ok(());
        }

        let Some(captures) = RE_VOTE.captures(&configstring) else {
            cold_path();
            let warning_str = format!("invalid vote called: {}", &configstring);
            dispatcher_debug_log(self.py(), &warning_str);
            return Ok(());
        };
        let vote = captures
            .name("cmd")
            .map(|value| value.as_str())
            .unwrap_or("");
        let args = captures
            .name("args")
            .map(|value| value.as_str())
            .unwrap_or("");
        let (yes_votes, no_votes) = MAIN_ENGINE.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ))
            },
            |main_engine| {
                Ok((
                    main_engine
                        .get_configstring(CS_VOTE_YES as u16)
                        .parse::<i32>()
                        .unwrap_or(0),
                    main_engine
                        .get_configstring(CS_VOTE_NO as u16)
                        .parse::<i32>()
                        .unwrap_or(0),
                ))
            },
        )?;

        let args_tuple = PyTuple::new(
            self.py(),
            [
                PyTuple::new(self.py(), [yes_votes, no_votes])?.into_any(),
                PyString::intern(self.py(), vote).into_any(),
                PyString::intern(self.py(), args).into_any(),
                PyBool::new(self.py(), passed).to_owned().into_any(),
            ],
        )?;

        self.as_super().dispatch(&args_tuple);

        Ok(())
    }
}

#[cfg(test)]
mod vote_ended_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::prelude::*;
    use rstest::rstest;

    use super::{VoteEndedDispatcher, VoteEndedDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES, CVar, CVarBuilder, cvar_t},
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
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
                });
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "0", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
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
                1,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck ca", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "0", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "8", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_vote_running(_pyshinqlx_setup: ()) {
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
            .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_unmatched_configstring(_pyshinqlx_setup: ()) {
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
            .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let dispatcher = Bound::new(py, VoteEndedDispatcher::py_new(py))
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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
                });
            });
    }
}
