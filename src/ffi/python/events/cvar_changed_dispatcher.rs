use pyo3::types::PyTuple;

use super::prelude::*;
use crate::ffi::python::events_enum::EventName;

/// Event that goes off after the value of a cvar changed. It cannot be cancelled, the value is set already.
#[pyclass(module = "_events", name = "CvarChangedDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct CvarChangedDispatcher {}

#[pymethods]
impl CvarChangedDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::CvarChanged.as_str();
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
        cvar_name: &str,
        old_value: &str,
        new_value: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(cvar_name, old_value, new_value)
    }
}

pub(crate) trait CvarChangedDispatcherMethods<'py> {
    fn dispatch(
        &self,
        cvar_name: &str,
        old_value: &str,
        new_value: &str,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> CvarChangedDispatcherMethods<'py> for Bound<'py, CvarChangedDispatcher> {
    fn dispatch(
        &self,
        cvar_name: &str,
        old_value: &str,
        new_value: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let args_tuple = PyTuple::new(self.py(), [cvar_name, old_value, new_value])?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod cvar_changed_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{CvarChangedDispatcher, CvarChangedDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::*,
            python::{
                commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
            },
        },
        prelude::*,
    };

    fn with_zmq_stats_engine(test: impl FnOnce()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(test);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_registered(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher =
                Bound::new(py, CvarChangedDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch("sv_hostname", "old", "new");
            assert!(result.is_ok_and(|value| {
                value
                    .cast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_hands_the_values_to_the_handlers(_pyshinqlx_setup: ()) {
        with_zmq_stats_engine(|| {
            Python::attach(|py| {
                let dispatcher = Bound::new(py, CvarChangedDispatcher::py_new(py))
                    .expect("this should not happen");
                let hook = PyModule::from_code(
                    py,
                    c"
seen = []

def hook(cvar_name, old_value, new_value):
    seen.append((cvar_name, old_value, new_value))
",
                    c"",
                    c"",
                )
                .expect("this should not happen");
                dispatcher
                    .as_super()
                    .add_hook(
                        "test_plugin",
                        &hook.getattr("hook").expect("this should not happen"),
                        CommandPriorities::PRI_NORMAL as i32,
                    )
                    .expect("this should not happen");

                dispatcher
                    .dispatch("sv_hostname", "old", "new")
                    .expect("this should not happen");

                assert_eq!(
                    hook.getattr("seen")
                        .expect("this should not happen")
                        .to_string(),
                    "[('sv_hostname', 'old', 'new')]"
                );
            });
        });
    }
}
