use super::prelude::*;

/// Event that goes off when someone is inflicted with damage.
#[pyclass(module = "_events", name = "DamageDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct DamageDispatcher {}

#[pymethods]
impl DamageDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "damage";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
