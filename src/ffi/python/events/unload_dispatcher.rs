use super::prelude::*;

/// Event that triggers whenever a plugin is unloaded. Cannot be cancelled.
#[pyclass(module = "_events", name = "UnloadDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UnloadDispatcher {}

#[pymethods]
impl UnloadDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "unload";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
