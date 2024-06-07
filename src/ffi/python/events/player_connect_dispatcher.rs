use super::prelude::*;

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

    fn dispatch(slf: &Bound<'_, Self>, player: PyObject) -> PyObject {
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        if let Ok(player_str) = player.bind(slf.py()).repr() {
            let dbgstr = format!("{}({})", Self::name, player_str);
            dispatcher_debug_log(slf.py(), &dbgstr);
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&player,)) {
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
                            return_value = str_value.clone().into_py(slf.py());
                        }
                    }
                }
            }
        }

        return_value
    }
}
