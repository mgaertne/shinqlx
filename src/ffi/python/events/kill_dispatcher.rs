use super::prelude::*;

/// Event that goes off when someone is killed.
#[pyclass(module = "_events", name = "KillDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KillDispatcher {}

#[pymethods]
impl KillDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kill";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
