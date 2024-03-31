use super::prelude::*;

/// Event that triggers whenever a plugin is unloaded. Cannot be cancelled.
#[pyclass(module = "_events", name = "UnloadDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UnloadDispatcher {}

#[pymethods]
impl UnloadDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "unload";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
