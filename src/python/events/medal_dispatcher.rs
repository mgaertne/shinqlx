use pyo3::{intern, types::PyTuple};

use super::prelude::*;
use crate::python::{Player, events_enum::EventName};

/// Event that goes off when a player is awarded a medal, with the name of the medal and the number of
/// medals of this kind the player holds now. It cannot be cancelled.
#[pyclass(module = "_events", name = "MedalDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct MedalDispatcher {}

#[pymethods]
impl MedalDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::Medal.as_str();
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
        medal: &str,
        total: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, medal, total)
    }
}

pub(crate) trait MedalDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        medal: &str,
        total: i32,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> MedalDispatcherMethods<'py> for Bound<'py, MedalDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        medal: &str,
        total: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py();
        let medal = py
            .import(intern!(py, "shinqlx"))
            .and_then(|module| module.getattr(intern!(py, "Medal")))
            .and_then(|class| class.call1((medal,)))
            .or_else(|_| medal.into_pyobject(py).map(|medal| medal.into_any()))?;
        let args_tuple = PyTuple::new(
            py,
            [
                player.as_any().clone(),
                medal,
                total.into_pyobject(py)?.into_any(),
            ],
        )?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod medal_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{ffi::c_str, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{MedalDispatcher, MedalDispatcherMethods};
    use crate::{
        engine::prelude::*,
        prelude::*,
        python::{
            commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
            pyshinqlx_test_support::default_test_player,
        },
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_with_no_handlers_returns_true(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let dispatcher =
                Bound::new(py, MedalDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                "IMPRESSIVE",
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
    fn dispatch_hands_medal_and_total_to_the_handler(_pyshinqlx_setup: ()) {
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
                    let dispatcher = Bound::new(py, MedalDispatcher::py_new(py))
                        .expect("this should not happen");
                    let hook = PyModule::from_code(
                        py,
                        c_str!(
                            "
def handler(player, medal, total):
    assert medal == 'EXCELLENT'
    assert total == 3
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
                        "EXCELLENT",
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
