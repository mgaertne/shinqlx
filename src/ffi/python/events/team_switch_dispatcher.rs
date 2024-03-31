use super::prelude::*;

/// For when a player switches teams. If cancelled,
/// simply put the player back in the old team.
///
/// If possible, consider using team_switch_attempt for a cleaner
/// solution if you need to cancel the event.
#[pyclass(module = "_events", name = "TeamSwitchDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct TeamSwitchDispatcher {}

#[pymethods]
impl TeamSwitchDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "team_switch";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            need_zmq_stats_enabled: true,
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
