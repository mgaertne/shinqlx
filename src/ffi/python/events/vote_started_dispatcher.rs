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

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(py: Python<'_>) -> (Self, EventDispatcher) {
        (Self { player: py.None() }, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, vote: &str, args: PyObject) -> bool {
        let mut return_value = true;

        let player = (&slf.borrow().player).into_py(slf.py());
        let super_class = slf.borrow().into_super();
        if let Ok(player_str) = player.bind(slf.py()).repr() {
            let dbgstr = format!("{}({}, {}, {})", Self::name, player_str, vote, &args);
            dispatcher_debug_log(slf.py(), &dbgstr);
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&player, vote, &args)) {
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

                            log_unexpected_return_value(slf.py(), Self::name, &res, handler);
                        }
                    }
                }
            }
        }

        return_value
    }

    pub(crate) fn caller(&mut self, _py: Python<'_>, player: PyObject) {
        self.player = player;
    }
}
