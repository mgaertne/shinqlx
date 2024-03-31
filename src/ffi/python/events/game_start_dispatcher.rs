use super::prelude::*;

/// Event that goes off when a game starts.
#[pyclass(module = "_events", name = "GameStartDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct GameStartDispatcher {}

#[pymethods]
impl GameStartDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "game_start";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
