use super::super::{Player, COMMANDS};
use super::prelude::*;
use crate::ffi::python::channels::ClientCommandChannel;
use pyo3::exceptions::PyEnvironmentError;

/// Event that triggers with any client command. This overlaps with
/// other events, such as "chat".
#[pyclass(module = "_events", name = "ClientCommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ClientCommandDispatcher {}

#[pymethods]
impl ClientCommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "client_command";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, player: Player, cmd: &str) -> PyObject {
        let mut forwarded_cmd = cmd.to_string();
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        let player_str = &player.name;
        let dbgstr = format!("{}({}, {})", Self::name, player_str, cmd);
        dispatcher_debug_log(py, &dbgstr);
        let plugins = super_class.plugins.read();

        let py_player = player.clone().into_py(py);
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(py, (&py_player, &forwarded_cmd)) {
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
                            forwarded_cmd.clone_from(&str_value);
                            return_value = str_value.clone().into_py(py);
                        }
                    }
                }
            }
        }

        if return_value.is_truthy(py).is_ok_and(|value| !value) {
            return false.into_py(py);
        }

        match try_handle_input(py, &player, cmd) {
            Err(e) => {
                log_exception(py, &e);
            }
            Ok(handle_input_return) => {
                if !handle_input_return {
                    return false.into_py(py);
                }
            }
        };

        return_value
    }
}

fn try_handle_input(py: Python<'_>, player: &Player, cmd: &str) -> PyResult<bool> {
    let client_command_channel = Py::new(py, ClientCommandChannel::py_new(player))?;
    COMMANDS.load().as_ref().map_or(
        Err(PyEnvironmentError::new_err(
            "could not get access to COMMANDS",
        )),
        |commands| {
            commands
                .borrow(py)
                .handle_input(py, player, cmd, client_command_channel.into_py(py))
        },
    )
}
