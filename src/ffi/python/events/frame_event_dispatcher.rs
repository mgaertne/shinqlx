use super::prelude::*;

/// Event that triggers every frame. Cannot be cancelled.
#[pyclass(module = "_events", name = "FrameEventDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct FrameEventDispatcher {}

#[pymethods]
impl FrameEventDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "frame";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
