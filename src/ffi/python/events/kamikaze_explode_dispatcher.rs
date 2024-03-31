use super::prelude::*;

/// Event that goes off when kamikaze explodes.
#[pyclass(module = "_events", name = "KamikazeExplodeDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KamikazeExplodeDispatcher {}

#[pymethods]
impl KamikazeExplodeDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kamikaze_explode";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
