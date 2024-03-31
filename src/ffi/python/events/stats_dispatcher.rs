use super::prelude::*;

/// Event that triggers whenever the server sends stats over ZMQ.
#[pyclass(module = "_events", name = "StatsDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct StatsDispatcher {}

#[pymethods]
impl StatsDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "stats";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
