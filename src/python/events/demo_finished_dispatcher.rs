use pyo3::types::PyTuple;

use super::prelude::*;
use crate::python::{Recording, events_enum::EventName};

/// Event that goes off when the file of a recording is written.
#[pyclass(module = "_events", name = "DemoFinishedDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct DemoFinishedDispatcher {}

#[pymethods]
impl DemoFinishedDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::DemoFinished.as_str();
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    fn py_new(_py: Python<'_>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(EventDispatcher::default()).add_subclass(Self {})
    }

    fn dispatch<'py>(
        slf: &Bound<'py, Self>,
        recording: &Bound<'py, Recording>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(recording)
    }
}

pub(crate) trait DemoFinishedDispatcherMethods<'py> {
    fn dispatch(&self, recording: &Bound<'py, Recording>) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> DemoFinishedDispatcherMethods<'py> for Bound<'py, DemoFinishedDispatcher> {
    fn dispatch(&self, recording: &Bound<'py, Recording>) -> PyResult<Bound<'py, PyAny>> {
        Ok(self
            .as_super()
            .dispatch(&PyTuple::new(self.py(), [recording])?))
    }
}
