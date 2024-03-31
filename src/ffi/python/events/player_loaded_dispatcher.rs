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
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
