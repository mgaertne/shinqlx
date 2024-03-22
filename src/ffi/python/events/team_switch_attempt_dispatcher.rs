use super::prelude::*;

/// For when a player attempts to join a team. Prevents the player from doing it when cancelled.
///
/// When players click the Join Match button, it sends "team a" (with the "a" being "any",
/// presumably), meaning the new_team argument can also be "any" in addition to all the
/// other teams.
#[pyclass(module = "_events", name = "TeamSwitchAttemptDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct TeamSwitchAttemptDispatcher {}

#[pymethods]
impl TeamSwitchAttemptDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "team_switch_attempt";

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
        old_team: String,
        new_team: String,
    ) -> bool {
        let mut return_value = true;

        let super_class = slf.into_super();
        if let Ok(player_str) = player.call_method0(py, intern!(py, "__repr__")) {
            let dbgstr = format!(
                "{}({}, {}, {})",
                super_class.name, player_str, &old_team, &new_team
            );
            dispatcher_debug_log(py, dbgstr);
        }

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&player, &old_team, &new_team)) {
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
