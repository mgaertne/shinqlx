use super::prelude::*;

/// Event that goes off when the countdown before a game starts.
#[pyclass(module = "_events", name = "GameCountdownDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct GameCountdownDispatcher {}

#[pymethods]
impl GameCountdownDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "game_countdown";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
