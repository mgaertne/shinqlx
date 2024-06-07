use super::prelude::*;

/// Event that triggers when the server tries to set a configstring. You can
/// stop this event and use :func:`shinqlx.set_configstring` to modify it, but a
/// more elegant way to do it is simply returning the new configstring in
/// the handler, and the modified one will go down the plugin chain instead.
#[pyclass(module = "_events", name = "SetConfigstringDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct SetConfigstringDispatcher {}

#[pymethods]
impl SetConfigstringDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "set_configstring";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, index: i32, value: &str) -> PyObject {
        let mut forwarded_value = value.to_string();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (index, &forwarded_value)) {
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
                            forwarded_value.clone_from(&str_value);
                            return_value = str_value.clone().into_py(slf.py());
                        }
                    }
                }
            }
        }

        return_value
    }
}
