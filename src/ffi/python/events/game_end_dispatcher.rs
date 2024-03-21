use super::prelude::*;

/// Event that goes off when a game ends.
#[pyclass(module = "_events", name = "GameEndDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct GameEndDispatcher {}

#[pymethods]
impl GameEndDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "game_end";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            need_zmq_stats_enabled: true,
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, data: PyObject) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        if let Ok(logger) = pyshinqlx_get_logger(py, None) {
            let mut dbgstr = format!("{}({})", super_class.name, &data);
            if dbgstr.len() > 100 {
                dbgstr.truncate(99);
                dbgstr.push(')');
            }
            if let Err(e) = logger.call_method1(intern!(py, "debug"), (dbgstr,)) {
                log_exception(py, e);
            };
        }
        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&data,)) {
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
}
