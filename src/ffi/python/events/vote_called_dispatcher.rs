use super::prelude::*;

/// Event that goes off whenever a player tries to call a vote. Note that
/// this goes off even if it's a vote command that is invalid. Use vote_started
/// if you only need votes that actually go through. Use this one for custom votes.
#[pyclass(module = "_events", name = "VoteCalledDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct VoteCalledDispatcher {}

#[pymethods]
impl VoteCalledDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "vote_called";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}
