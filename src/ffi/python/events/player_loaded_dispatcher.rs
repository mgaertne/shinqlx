use super::prelude::*;

/// Event that triggers whenever a player connects *and* finishes loading.
/// This means it'll trigger later than the "X connected" messages in-game,
/// and it will also trigger when a map changes and players finish loading it.
#[pyclass(module = "_events", name = "PlayerLoadedDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct PlayerLoadedDispatcher {}

#[pymethods]
impl PlayerLoadedDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "player_loaded";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, player: PyObject) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        if let Ok(logger) = pyshinqlx_get_logger(py, None) {
            if let Ok(player_str) = player.call_method0(py, intern!(py, "__str__")) {
                let mut dbgstr = format!("{}({})", super_class.name, player_str);
                if dbgstr.len() > 100 {
                    dbgstr.truncate(99);
                    dbgstr.push(')');
                }
                if let Err(e) = logger.call_method1(intern!(py, "debug"), (dbgstr,)) {
                    log_exception(py, e);
                };
            }
        }
        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player,)) {
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