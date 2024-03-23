use super::prelude::*;

/// Event that goes off when a map is loaded, even if the same map is loaded again.
#[pyclass(module = "_events", name = "MapDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct MapDispatcher {}

#[pymethods]
impl MapDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "map";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, mapname: String, factory: String) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        let dbgstr = format!("{}({}, {})", super_class.name, &mapname, &factory);
        dispatcher_debug_log(py, dbgstr);

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&mapname, &factory)) {
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
