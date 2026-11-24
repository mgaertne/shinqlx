use pyo3::{intern, types::PyTuple};

use super::prelude::*;
use crate::ffi::python::{Player, events_enum::EventName};

/// Event that goes off when a flag changes its state: the `team` of the flag (`"red"`, `"blue"` or
/// `"neutral"`), its `status` (`"at_base"`, `"taken"` or `"dropped"`) and the `Player` that carries
/// or dropped it, or `None`. It cannot be cancelled.
#[pyclass(module = "_events", name = "FlagStatusDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct FlagStatusDispatcher {}

#[pymethods]
impl FlagStatusDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::FlagStatus.as_str();
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
        team: &str,
        status: &str,
        player: Option<&Bound<'py, Player>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(team, status, player)
    }
}

pub(crate) trait FlagStatusDispatcherMethods<'py> {
    fn dispatch(
        &self,
        team: &str,
        status: &str,
        player: Option<&Bound<'py, Player>>,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> FlagStatusDispatcherMethods<'py> for Bound<'py, FlagStatusDispatcher> {
    fn dispatch(
        &self,
        team: &str,
        status: &str,
        player: Option<&Bound<'py, Player>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = self.py();
        let to_member = |class: &str, value: &str| {
            py.import(intern!(py, "shinqlx"))
                .and_then(|module| module.getattr(class))
                .and_then(|class| class.call1((value,)))
                .or_else(|_| value.into_pyobject(py).map(|value| value.into_any()))
        };
        let team = to_member("FlagTeam", team)?;
        let status = to_member("FlagStatus", status)?;
        let player = match player {
            Some(player) => player.as_any().clone(),
            None => py.None().into_bound(py),
        };
        let args_tuple = PyTuple::new(py, [team, status, player])?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod flag_status_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{ffi::c_str, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{FlagStatusDispatcher, FlagStatusDispatcherMethods};
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
                Bound::new(py, FlagStatusDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch("red", "at_base", None);
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
    fn dispatch_hands_team_status_and_carrier_to_the_handler(_pyshinqlx_setup: ()) {
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
                    let dispatcher = Bound::new(py, FlagStatusDispatcher::py_new(py))
                        .expect("this should not happen");
                    let hook = PyModule::from_code(
                        py,
                        c_str!(
                            "
def handler(team, status, player):
    assert team == 'blue'
    assert status == 'taken'
    assert player is not None
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
                        "blue",
                        "taken",
                        Some(
                            &Bound::new(py, default_test_player()).expect("this should not happen"),
                        ),
                    );
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Bound<'_, PyBool>>()
                            .is_ok_and(|value| value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dispatch_without_carrier_hands_none_to_the_handler(_pyshinqlx_setup: ()) {
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
                    let dispatcher = Bound::new(py, FlagStatusDispatcher::py_new(py))
                        .expect("this should not happen");
                    let hook = PyModule::from_code(
                        py,
                        c_str!(
                            "
def handler(team, status, player):
    assert player is None
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

                    let result = dispatcher.dispatch("red", "dropped", None);
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Bound<'_, PyBool>>()
                            .is_ok_and(|value| value.is_true())
                    }));
                });
            });
    }
}
