use super::prelude::*;

/// Event that goes off when a round starts.
#[pyclass(module = "_events", name = "RoundStartDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct RoundStartDispatcher {}

#[pymethods]
impl RoundStartDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "round_start";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
