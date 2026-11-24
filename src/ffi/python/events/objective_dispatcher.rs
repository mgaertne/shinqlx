use pyo3::{intern, types::PyTuple};

use super::prelude::*;
use crate::ffi::python::{Player, events_enum::EventName};

/// Event that goes off when a player scores an objective of a team mode. It cannot be cancelled.
#[pyclass(module = "_events", name = "ObjectiveDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ObjectiveDispatcher {}

#[pymethods]
impl ObjectiveDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::Objective.as_str();
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const need_zmq_stats_enabled: bool = false;

    #[new]
    pub(crate) fn py_new(_py: Python<'_>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(EventDispatcher::default()).add_subclass(Self {})
    }

    fn dispatch<'py>(
        slf: &Bound<'py, Self>,
        player: &Bound<'py, Player>,
        kind: &str,
        count: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, kind, count)
    }
}

pub(crate) trait ObjectiveDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        kind: &str,
        count: i32,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> ObjectiveDispatcherMethods<'py> for Bound<'py, ObjectiveDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        kind: &str,
        count: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py();
        let objective = py
            .import(intern!(py, "shinqlx"))
            .and_then(|module| module.getattr(intern!(py, "Objective")))
            .and_then(|objective| objective.call1((kind,)))
            .or_else(|_| kind.into_pyobject(py).map(|kind| kind.into_any()))?;
        let args_tuple = PyTuple::new(
            py,
            [
                player.as_any().clone(),
                objective,
                count.into_pyobject(py)?.into_any(),
            ],
        )?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod objective_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{ffi::c_str, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{ObjectiveDispatcher, ObjectiveDispatcherMethods};
    use crate::{
        ffi::{
            c::prelude::*,
            python::{
                commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
                pyshinqlx_test_support::default_test_player,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_returns_true(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher =
                Bound::new(py, ObjectiveDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                "capture",
                1,
            );
            assert!(result.is_ok_and(|value| {
                value
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|value| value.is_true())
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_hands_kind_and_count_to_the_handler(_pyshinqlx_setup: ()) {
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
            .run(|| {
                Python::attach(|py| {
                    let dispatcher = Bound::new(py, ObjectiveDispatcher::py_new(py))
                        .expect("this should not happen");
                    let hook = PyModule::from_code(
                        py,
                        c_str!(
                            "
def handler(player, kind, count):
    assert kind == 'base_defense'
    assert count == 3
    return None
"
                        ),
                        c"",
                        c"",
                    )
                    .and_then(|module| module.getattr("handler"))
                    .expect("this should not happen");
                    dispatcher
                        .as_super()
                        .add_hook("test_plugin", &hook, CommandPriorities::PRI_NORMAL as i32)
                        .expect("this should not happen");

                    let result = dispatcher.dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        "base_defense",
                        3,
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Bound<'_, PyBool>>()
                            .is_ok_and(|value| value.is_true())
                    }));
                });
            });
    }
}
