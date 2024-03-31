use super::prelude::*;

/// Event that goes off when a command is executed. This can be used
/// to for instance keep a log of all the commands admins have used.
#[pyclass(module = "_events", name = "CommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct CommandDispatcher {}

#[pymethods]
impl CommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "command";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.to_string(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }
}
