use super::prelude::*;

/// Event that goes off when a map is loaded, even if the same map is loaded again.
#[pyclass(module = "_events", name = "MapDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct MapDispatcher {}

#[pymethods]
impl MapDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "map";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
