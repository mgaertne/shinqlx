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

    fn dispatch(slf: &Bound<'_, Self>, text: &str) -> PyObject {
        let mut forwarded_text = text.to_string();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&forwarded_text,)) {
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
                            forwarded_text.clone_from(&str_value);
                            return_value = str_value.clone().into_py(slf.py());
                        }
                    }
                }
            }
        }

        return_value
    }
}
