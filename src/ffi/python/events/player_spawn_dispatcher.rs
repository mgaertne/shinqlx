use super::prelude::*;

/// Event that triggers when a player spawns. Cannot be cancelled.
#[pyclass(module = "_events", name = "PlayerSpawnDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct PlayerSpawnDispatcher {}

#[pymethods]
impl PlayerSpawnDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "player_spawn";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
