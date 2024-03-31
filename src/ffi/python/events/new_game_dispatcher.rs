use super::prelude::*;

/// Event that goes off when the game module is initialized. This happens when new maps are loaded,
/// a game is aborted, a game ends but stays on the same map, or when the game itself starts.
#[pyclass(module = "_events", name = "NewGameDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct NewGameDispatcher {}

#[pymethods]
impl NewGameDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "new_game";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
