use super::prelude::*;

/// Event that goes off when kamikaze explodes.
#[pyclass(module = "_events", name = "KamikazeExplodeDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KamikazeExplodeDispatcher {}

#[pymethods]
impl KamikazeExplodeDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kamikaze_explode";

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
        is_used_on_demand: bool,
    ) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        if let Ok(player_str) = player.call_method0(py, intern!(py, "__repr__")) {
            let dbgstr = format!(
                "{}({}, {})",
                super_class.name, player_str, is_used_on_demand
            );
            dispatcher_debug_log(py, dbgstr);
        }

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, is_used_on_demand)) {
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
