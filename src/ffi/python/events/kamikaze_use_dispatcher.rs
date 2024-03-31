use super::prelude::*;

/// Event that goes off when player uses kamikaze item.
#[pyclass(module = "_events", name = "KamikazeUseDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct KamikazeUseDispatcher {}

#[pymethods]
impl KamikazeUseDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "kamikaze_use";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
