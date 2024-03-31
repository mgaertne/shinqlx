use super::prelude::*;

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

    fn dispatch(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        msg: &str,
        channel: PyObject,
    ) -> PyObject {
        match try_handle_input(py, &player, msg, &channel) {
            Err(e) => {
                log_exception(py, &e);
            }
            Ok(handle_input_return) => {
                if handle_input_return.is_truthy(py).is_ok_and(|value| !value) {
                    return false.into_py(py);
                }
            }
        };

        let mut forwarded_msg = msg.to_string();
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        if let Ok(player_str) = player.bind(py).repr() {
            if let Ok(channel_str) = channel.bind(py).repr() {
                let dbgstr = format!(
                    "{}({}, {}, {})",
                    Self::name, player_str, msg, channel_str
                );
                dispatcher_debug_log(py, &dbgstr);
            }
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, &forwarded_msg, &channel)) {
                        Err(e) => {
                            log_exception(py, &e);
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
                                return true.into_py(py);
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                            {
                                return_value = false.into_py(py);
                                continue;
                            }
                            if res_i32
                                .as_ref()
                                .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL)
                            {
                                return false.into_py(py);
                            }

                            let Ok(str_value) = res.extract::<String>(py) else {
                                log_unexpected_return_value(py, Self::name, &res, handler);
                                continue;
                            };
                            forwarded_msg.clone_from(&str_value);
                            return_value = str_value.clone().into_py(py);
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
    player: &PyObject,
    cmd: &str,
    channel: &PyObject,
) -> PyResult<PyObject> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let commands = shinqlx_module.getattr(intern!(py, "COMMANDS"))?;
    commands
        .call_method1(intern!(py, "handle_input"), (player, cmd, channel))
        .map(|ret| ret.into_py(py))
}
