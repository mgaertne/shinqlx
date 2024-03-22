use super::prelude::*;

/// Event that goes off whenever a vote starts. A vote started with Plugin.callvote()
/// will have the caller set to None.
#[pyclass(module = "_events", name = "VoteStartedDispatcher", extends = EventDispatcher)]
pub(crate) struct VoteStartedDispatcher {
    player: PyObject,
}

#[pymethods]
impl VoteStartedDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote_started";

    #[new]
    fn py_new(py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self { player: py.None() }, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, vote: String, args: PyObject) -> bool {
        let mut return_value = true;

        let player = (&slf.player).into_py(py);
        let super_class = slf.into_super();
        if let Ok(player_str) = player.call_method0(py, intern!(py, "__repr__")) {
            let dbgstr = format!("{}({}, {}, {})", super_class.name, player_str, &vote, &args);
            dispatcher_debug_log(py, dbgstr);
        }

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, &vote, &args)) {
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
                                return true;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                return_value = false;
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return false;
                            }

                            log_unexpected_return_value(py, Self::name, &res, handler);
                        }
                    }
                }
            }
        }

        return_value
    }

    fn caller(&mut self, _py: Python<'_>, player: PyObject) {
        self.player = player;
    }
}
