use super::prelude::*;

/// Event that goes off when someone dies.
#[pyclass(module = "_events", name = "DeathDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct DeathDispatcher {}

#[pymethods]
impl DeathDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "death";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = true;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
