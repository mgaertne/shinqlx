use super::prelude::*;

/// Event that goes off when player uses kamikaze item.
#[pyclass(module = "_events", name = "KamikazeUseDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KamikazeUseDispatcher {}

#[pymethods]
impl KamikazeUseDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kamikaze_use";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
