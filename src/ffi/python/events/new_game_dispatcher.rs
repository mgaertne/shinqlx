use super::prelude::*;

/// Event that goes off when the game module is initialized. This happens when new maps are loaded,
/// a game is aborted, a game ends but stays on the same map, or when the game itself starts.
#[pyclass(module = "_events", name = "NewGameDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct NewGameDispatcher {}

#[pymethods]
impl NewGameDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "new_game";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        let dbgstr = format!("{}()", super_class.name);
        dispatcher_debug_log(py, &dbgstr);

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call0(py) {
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
