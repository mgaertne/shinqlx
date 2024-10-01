use super::prelude::*;

use crate::{
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES},
    quake_live_engine::GetConfigstring,
    MAIN_ENGINE,
};

use once_cell::sync::Lazy;
use pyo3::types::PyTuple;
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

    fn dispatch(slf: &Bound<'_, Self>, passed: bool) {
        MAIN_ENGINE.load().iter().for_each(|main_engine| {
            let configstring = main_engine.get_configstring(CS_VOTE_STRING as u16);
            if configstring.is_empty() {
                dispatcher_debug_log(
                    slf.py(),
                    "vote_ended went off without configstring CS_VOTE_STRING.",
                );
                return;
            }

            let Some(captures) = RE_VOTE.captures(&configstring) else {
                let warning_str = format!("invalid vote called: {}", &configstring);
                dispatcher_debug_log(slf.py(), &warning_str);
                return;
            };
            let vote = captures
                .name("cmd")
                .map(|value| value.as_str())
                .unwrap_or("");
            let args = captures
                .name("args")
                .map(|value| value.as_str())
                .unwrap_or("");
            let yes_votes = main_engine
                .get_configstring(CS_VOTE_YES as u16)
                .parse::<i32>()
                .unwrap_or(0);
            let no_votes = main_engine
                .get_configstring(CS_VOTE_NO as u16)
                .parse::<i32>()
                .unwrap_or(0);

            let pyany_event_dispatcher = slf.borrow().into_super().into_py(slf.py());
            pyany_event_dispatcher
                .bind(slf.py())
                .downcast()
                .iter()
                .for_each(|event_dispatcher_instance| {
                    let args_tuple = PyTuple::new_bound(
                        slf.py(),
                        [
                            (yes_votes, no_votes).into_py(slf.py()),
                            vote.into_py(slf.py()),
                            args.into_py(slf.py()),
                            passed.into_py(slf.py()),
                        ],
                    );

                    EventDispatcher::dispatch(event_dispatcher_instance, args_tuple);
                });
        })
    }
}

#[cfg(test)]
mod vote_ended_dispatcher_tests {
    use super::VoteEndedDispatcher;

    use crate::ffi::c::prelude::{
        cvar_t, CVar, CVarBuilder, CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES,
    };
    use crate::ffi::python::{commands::CommandPriorities, pyshinqlx_setup};
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_when_handler_returns_exception(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
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
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck ca".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_vote_running(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
        });

        MAIN_ENGINE.store(None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_unmatched_configstring(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning_st(move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| " ".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "0".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "8".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, VoteEndedDispatcher::py_new(py)).expect("this should not happen");

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

            let result = dispatcher.call_method1(py, intern!(py, "dispatch"), (true,));
            assert!(result.is_ok_and(|value| value.bind(py).is_none()));
        });

        MAIN_ENGINE.store(None);
    }
}
