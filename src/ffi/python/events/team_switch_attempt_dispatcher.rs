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

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
