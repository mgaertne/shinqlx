use super::prelude::*;

/// Event that goes off when player uses kamikaze item.
#[pyclass(module = "_events", name = "KamikazeUseDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KamikazeUseDispatcher {}

#[pymethods]
impl KamikazeUseDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kamikaze_use";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, player: PyObject) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        if let Ok(player_str) = player.bind(py).repr() {
            let dbgstr = format!("{}({})", super_class.name, player_str);
            dispatcher_debug_log(py, dbgstr);
        }

        let plugins = super_class.plugins.read();
        for i in 0..5 {
            for (_, handlers) in plugins.iter() {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player,)) {
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