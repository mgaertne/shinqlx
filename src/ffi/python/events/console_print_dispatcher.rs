use super::prelude::*;

/// Event that goes off whenever the console prints something, including
/// those with :func:`shinqlx.console_print`.
#[pyclass(module = "_events", name = "ConsolePrintDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ConsolePrintDispatcher {}

#[pymethods]
impl ConsolePrintDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "console_print";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, text: &str) -> PyObject {
        let mut forwarded_text = text.to_string();
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(py, (&forwarded_text,)) {
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
                            forwarded_text.clone_from(&str_value);
                            return_value = str_value.clone().into_py(py);
                        }
                    }
                }
            }
        }

        return_value
    }
}
