use super::prelude::*;

/// Event that goes off when someone is inflicted with damage.
#[pyclass(module = "_events", name = "DamageDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct DamageDispatcher {}

#[pymethods]
impl DamageDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "damage";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
