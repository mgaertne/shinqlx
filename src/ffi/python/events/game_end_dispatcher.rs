use super::prelude::*;

/// Event that goes off when a game ends.
#[pyclass(module = "_events", name = "GameEndDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct GameEndDispatcher {}

#[pymethods]
impl GameEndDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "game_end";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
