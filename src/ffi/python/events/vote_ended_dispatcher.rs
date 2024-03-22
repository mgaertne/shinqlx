use super::prelude::*;

use crate::{
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES},
    quake_live_engine::GetConfigstring,
    MAIN_ENGINE,
};

use once_cell::sync::Lazy;
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

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, passed: bool) {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return;
        };
        let configstring = main_engine.get_configstring(CS_VOTE_STRING as u16);
        if configstring.is_empty() {
            dispatcher_debug_log(
                py,
                "vote_ended went off without configstring CS_VOTE_STRING.".into(),
            );
            return;
        }

        let Some(captures) = RE_VOTE.captures(&configstring) else {
            let warning_str = format!("invalid vote called: {}", &configstring);
            dispatcher_debug_log(py, warning_str);
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

        let super_class = slf.into_super();
        let dbgstr = format!(
            "{}(({}, {}), {}, {}, {})",
            super_class.name, yes_votes, no_votes, vote, args, passed
        );
        dispatcher_debug_log(py, dbgstr);

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, ((yes_votes, no_votes), vote, args, passed)) {
                        Err(e) => {
                            log_exception(py, e);
                            continue;
                        }
                        Ok(res) => {
                            let res_i32 = res.extract::<PythonReturnCodes>(py);
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
                                return;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return;
                            }

                            log_unexpected_return_value(py, Self::name, &res, handler);
                        }
                    }
                }
            }
        }
    }
}
