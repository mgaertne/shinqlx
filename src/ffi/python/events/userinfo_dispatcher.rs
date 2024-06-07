use super::prelude::*;

use pyo3::types::{IntoPyDict, PyDict};

/// Event for clients changing their userinfo.
#[pyclass(module = "_events", name = "UserinfoDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UserinfoDispatcher {}

#[pymethods]
impl UserinfoDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "userinfo";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }

    fn dispatch(slf: &Bound<'_, Self>, player: PyObject, changed: &Bound<'_, PyDict>) -> PyObject {
        let mut forwarded_userinfo = changed.clone();
        let mut return_value = true.into_py(slf.py());

        let super_class = slf.borrow().into_super();
        if let Ok(player_str) = player.bind(slf.py()).repr() {
            if let Ok(changed_str) = changed.repr() {
                let dbgstr = format!("{}({}, {})", Self::name, player_str, changed_str);
                dispatcher_debug_log(slf.py(), &dbgstr);
            }
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(slf.py(), (&player, forwarded_userinfo.clone())) {
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

                            let Ok(changed_value) = res.extract::<&PyDict>(slf.py()) else {
                                log_unexpected_return_value(slf.py(), Self::name, &res, handler);
                                continue;
                            };
                            forwarded_userinfo = changed_value.into_py_dict_bound(slf.py());
                            return_value = changed_value.into_py(slf.py());
                        }
                    }
                }
            }
        }

        return_value
    }
}
