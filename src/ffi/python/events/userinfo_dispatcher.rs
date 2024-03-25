use super::prelude::*;

use pyo3::types::PyDict;

/// Event for clients changing their userinfo.
#[pyclass(module = "_events", name = "UserinfoDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UserinfoDispatcher {}

#[pymethods]
impl UserinfoDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "userinfo";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        changed: &PyDict,
    ) -> PyObject {
        let mut forwarded_userinfo = changed;
        let mut return_value = true.into_py(py);

        let super_class = slf.into_super();
        if let Ok(player_str) = player.call_method0(py, intern!(py, "__repr__")) {
            if let Ok(changed_str) = changed.call_method0(intern!(py, "__repr__")) {
                let dbgstr = format!("{}({}, {})", super_class.name, player_str, changed_str);
                dispatcher_debug_log(py, dbgstr);
            }
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, &forwarded_userinfo)) {
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

                            let Ok(changed_value) = res.extract::<&PyDict>(py) else {
                                log_unexpected_return_value(py, Self::name, &res, handler);
                                continue;
                            };
                            forwarded_userinfo = changed_value;
                            return_value = changed_value.into_py(py);
                        }
                    }
                }
            }
        }

        return_value
    }
}
