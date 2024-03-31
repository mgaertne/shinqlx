use super::prelude::*;

/// Event that goes off when a round starts.
#[pyclass(module = "_events", name = "RoundStartDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct RoundStartDispatcher {}

#[pymethods]
impl RoundStartDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "round_start";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
