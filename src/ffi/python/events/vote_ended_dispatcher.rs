use super::prelude::*;

use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES},
    quake_live_engine::GetConfigstring,
};

use once_cell::sync::Lazy;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::types::{PyBool, PyString, PyTuple};

use regex::Regex;

static RE_VOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^(?P<cmd>[^ ]+)(?: "?(?P<args>.*?)"?)?$"#).unwrap());

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
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| Ok(main_engine.get_configstring(CS_VOTE_STRING as u16)),
        )?;

        if configstring.is_empty() {
            dispatcher_debug_log(
                self.py(),
                "vote_ended went off without configstring CS_VOTE_STRING.",
            );
            return Ok(());
        }

        let Some(captures) = RE_VOTE.captures(&configstring) else {
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
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
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
                PyString::new(self.py(), vote).into_any(),
                PyString::new(self.py(), args).into_any(),
                PyBool::new(self.py(), passed).to_owned().into_any(),
            ],
        )?;

        self.as_super().dispatch(&args_tuple);

        Ok(())
    }
}

#[cfg(test)]
mod vote_ended_dispatcher_tests {
    use super::{VoteEndedDispatcher, VoteEndedDispatcherMethods};

    use crate::ffi::c::prelude::{
        CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES, CVar, CVarBuilder, cvar_t,
    };
    use crate::ffi::python::{
        commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
    };
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::rstest;

    use pyo3::prelude::*;

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

                    let result = dispatcher.dispatch(true);
                    assert!(result.is_ok());
                });
            });
    }
}
