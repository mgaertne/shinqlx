use super::prelude::*;

/// Event that goes off when the countdown before a game starts.
#[pyclass(module = "_events", name = "GameCountdownDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct GameCountdownDispatcher {}

#[pymethods]
impl GameCountdownDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "game_countdown";

    #[classattr]
    #[allow(non_upper_case_globals)]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        (Self {}, EventDispatcher::default())
    }
}

#[cfg(test)]
mod game_countdown_dispatcher_tests {
    use super::GameCountdownDispatcher;

    use crate::ffi::python::pyshinqlx_setup;

    use rstest::rstest;

    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::PyTuple;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dispatch_with_no_plugins_registered(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let dispatcher =
                Py::new(py, GameCountdownDispatcher::py_new(py)).expect("this should not happen");

            let result =
                dispatcher.call_method1(py, intern!(py, "dispatch"), PyTuple::empty_bound(py));
            assert!(result.is_ok());
        });
    }
}
