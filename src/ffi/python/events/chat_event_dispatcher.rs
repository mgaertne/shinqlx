use super::prelude::*;
use pyo3::exceptions::PyEnvironmentError;

use super::super::{Player, COMMANDS};

/// Event that triggers with the "say" command. If the handler cancels it,
/// the message will also be cancelled.
#[pyclass(module = "_events", name = "ChatEventDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ChatEventDispatcher {}

#[pymethods]
impl ChatEventDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "chat";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, player: Player, msg: &str, channel: PyObject) -> PyObject {
        match try_handle_input(slf.py(), &player, msg, channel.clone_ref(slf.py())) {
            Err(e) => {
                log_exception(slf.py(), &e);
            }
            Ok(handle_input_return) => {
                if !handle_input_return {
                    return false.into_py(slf.py());
                }
            }
        };

        let mut forwarded_msg = msg.to_string();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        let player_str = &player.name;
        if let Ok(channel_str) = channel.bind(slf.py()).repr() {
            let dbgstr = format!("{}({}, {}, {})", Self::name, player_str, msg, channel_str);
            dispatcher_debug_log(slf.py(), &dbgstr);
        }

        let plugins = super_class.plugins.read();
        let py_player = player.into_py(slf.py());
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&py_player, &forwarded_msg, &channel)) {
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
                            forwarded_msg.clone_from(&str_value);
                            return_value = str_value.clone().into_py(slf.py());
                        }
                    }
                }
            }
        }

        return_value
    }
}

fn try_handle_input(
    py: Python<'_>,
    player: &Player,
    cmd: &str,
    channel: PyObject,
) -> PyResult<bool> {
    COMMANDS.load().as_ref().map_or(
        Err(PyEnvironmentError::new_err(
            "could not get access to COMMANDS",
        )),
        |commands| commands.borrow(py).handle_input(py, player, cmd, channel),
    )
}
