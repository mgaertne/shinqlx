use super::prelude::*;

/// Event that goes off when the countdown before a round starts.
#[pyclass(module = "_events", name = "RoundCountdownDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct RoundCountdownDispatcher {}

#[pymethods]
impl RoundCountdownDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "round_countdown";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
