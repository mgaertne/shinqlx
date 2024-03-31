use super::prelude::*;

/// Event that goes off whenever someone tries to vote either yes or no.
#[pyclass(module = "_events", name = "VoteDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteDispatcher {}

#[pymethods]
impl VoteDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
