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

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, index: i32, value: String) -> PyObject {
        let mut forwarded_value = value.clone();
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (index, &forwarded_value)) {
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
                            forwarded_value.clone_from(&str_value);
                            return_value = str_value.clone().into_py(py);
                        }
                    }
                }
            }
        }

        return_value
    }
}
