use super::prelude::*;

/// Event that triggers when a player spawns. Cannot be cancelled.
#[pyclass(module = "_events", name = "PlayerSpawnDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct PlayerSpawnDispatcher {}

#[pymethods]
impl PlayerSpawnDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "player_spawn";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
