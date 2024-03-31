use super::prelude::*;

/// Event that goes off when a round ends.
#[pyclass(module = "_events", name = "RoundEndDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct RoundEndDispatcher {}

#[pymethods]
impl RoundEndDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "round_end";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
