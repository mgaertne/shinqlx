use super::prelude::*;

/// Event that goes off whenever someone tries to vote either yes or no.
#[pyclass(module = "_events", name = "VoteDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteDispatcher {}

#[pymethods]
impl VoteDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
