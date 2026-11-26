use pyo3::types::PyTuple;

use super::prelude::*;
use crate::python::{Player, events_enum::EventName};

/// Event that goes off after a player has picked an item up. It cannot be cancelled, the item is gone.
#[pyclass(module = "_events", name = "ItemPickupDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct ItemPickupDispatcher {}

#[pymethods]
impl ItemPickupDispatcher {
    #[classattr]
    #[allow(
        non_upper_case_globals,
        reason = "exposed to Python as a lower-case class attribute"
    )]
    const name: &'static str = EventName::ItemPickup.as_str();
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
        player: &Bound<'py, Player>,
        entity_id: i32,
        classname: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.dispatch(player, entity_id, classname)
    }
}

pub(crate) trait ItemPickupDispatcherMethods<'py> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        entity_id: i32,
        classname: &str,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> ItemPickupDispatcherMethods<'py> for Bound<'py, ItemPickupDispatcher> {
    fn dispatch(
        &self,
        player: &Bound<'py, Player>,
        entity_id: i32,
        classname: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let args_tuple = PyTuple::new(
            self.py(),
            [
                player.as_any().clone(),
                entity_id.into_pyobject(self.py())?.into_any(),
                classname.into_pyobject(self.py())?.into_any(),
            ],
        )?;
        Ok(self.as_super().dispatch(&args_tuple))
    }
}

#[cfg(test)]
mod item_pickup_dispatcher_tests {
    use core::borrow::BorrowMut;

    use pyo3::{prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{ItemPickupDispatcher, ItemPickupDispatcherMethods};
    use crate::{
        engine::prelude::*,
        prelude::*,
        python::{
            commands::CommandPriorities, events::EventDispatcherMethods, pyshinqlx_setup,
            pyshinqlx_test_support::default_test_player,
        },
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
                Bound::new(py, ItemPickupDispatcher::py_new(py)).expect("this should not happen");

            let result = dispatcher.dispatch(
                &Bound::new(py, default_test_player()).expect("this should not happen"),
                42,
                "item_armor_body",
            );
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
    fn dispatch_hands_the_item_to_the_handlers(_pyshinqlx_setup: ()) {
        with_zmq_stats_engine(|| {
            Python::attach(|py| {
                let dispatcher = Bound::new(py, ItemPickupDispatcher::py_new(py))
                    .expect("this should not happen");
                let hook = PyModule::from_code(
                    py,
                    c"
seen = []

def hook(player, entity_id, classname):
    seen.append((entity_id, classname))
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
                    .dispatch(
                        &Bound::new(py, default_test_player()).expect("this should not happen"),
                        42,
                        "item_armor_body",
                    )
                    .expect("this should not happen");

                assert_eq!(
                    hook.getattr("seen")
                        .expect("this should not happen")
                        .to_string(),
                    "[(42, 'item_armor_body')]"
                );
            });
        });
    }
}
