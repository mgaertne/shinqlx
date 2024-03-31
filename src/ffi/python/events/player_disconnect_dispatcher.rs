use super::prelude::*;

/// Event that triggers whenever a player disconnects. Cannot be cancelled.
#[pyclass(module = "_events", name = "PlayerDisconnectDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct PlayerDisconnectDispatcher {}

#[pymethods]
impl PlayerDisconnectDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "player_disconnect";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
