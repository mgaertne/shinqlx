use super::prelude::*;

/// Event that triggers every frame. Cannot be cancelled.
#[pyclass(module = "_events", name = "FrameEventDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct FrameEventDispatcher {}

#[pymethods]
impl FrameEventDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "frame";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
