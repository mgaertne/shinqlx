use super::prelude::*;

/// Event that triggers with any client command. This overlaps with
/// other events, such as "chat".
#[pyclass(module = "_events", name = "ClientCommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ClientCommandDispatcher {}

#[pymethods]
impl ClientCommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "client_command";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, player: PyObject, cmd: String) -> PyObject {
        let mut forwarded_cmd = cmd.clone();
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        if let Ok(logger) = pyshinqlx_get_logger(py, None) {
            if let Ok(player_str) = player.call_method0(py, intern!(py, "__str__")) {
                let mut dbgstr = format!("{}({}, {})", super_class.name, player_str, cmd);
                if dbgstr.len() > 100 {
                    dbgstr.truncate(99);
                    dbgstr.push(')');
                }
                if let Err(e) = logger.call_method1(intern!(py, "debug"), (dbgstr,)) {
                    log_exception(py, e);
                };
            }
        }
        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, &forwarded_cmd)) {
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

        match try_handle_input(py, &player, &cmd) {
            Err(e) => {
                log_exception(py, e);
            }
            Ok(handle_input_return) => {
                if handle_input_return.is_truthy(py).is_ok_and(|value| !value) {
                    return false.into_py(py);
                }
            }
        };

        return_value
    }
}

fn try_handle_input(py: Python<'_>, player: &PyObject, cmd: &String) -> PyResult<PyObject> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let client_command_channel =
        shinqlx_module.call_method1(intern!(py, "ClientCommandChannel"), (player,))?;
    let commands = shinqlx_module.getattr(intern!(py, "COMMANDS"))?;
    commands
        .call_method1(
            intern!(py, "handle_input"),
            (player, cmd, client_command_channel),
        )
        .map(|ret| ret.into_py(py))
}