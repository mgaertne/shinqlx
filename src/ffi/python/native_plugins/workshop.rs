use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyTuple},
};

use super::{create_cvar, register_hook};
use crate::ffi::python::{
    commands::CommandPriorities,
    events_enum::EventName,
    game::GameMethods,
    plugin::{Plugin, PluginMethods, cvar_items},
};

/// Adds the workshop items of the cvar `qlx_workshopReferences` to every map, and to the running one when it is loaded.
#[pyclass(name = "workshop", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Workshop;

#[pymethods]
impl Workshop {
    #[pyo3(signature = (*_args, **_kwargs))]
    fn handle_map(
        slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        add_references(slf)
    }
}

fn add_references(plugin: &Bound<'_, Workshop>) -> PyResult<()> {
    let py = plugin.py();
    let Some(game) = plugin.as_super().get_game() else {
        return Ok(());
    };
    let game = Bound::new(py, game)?;

    let present = game.get_workshop_items()?;
    let missing_references = cvar_items("qlx_workshopReferences")
        .into_iter()
        .filter(|reference| {
            !reference
                .parse::<u64>()
                .is_ok_and(|id| present.contains(&id))
        });
    let workshop_items = present
        .iter()
        .map(u64::to_string)
        .chain(missing_references)
        .collect::<Vec<_>>();
    game.set_workshop_items(&PyList::new(py, workshop_items)?.into_any())
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, Plugin>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Workshop),
    )?;
    register_hook(
        plugin.as_super(),
        EventName::Map,
        "handle_map",
        CommandPriorities::PRI_NORMAL,
    )?;
    create_cvar(py, "qlx_workshopReferences", "")?;
    add_references(&plugin)?;

    Ok(plugin.into_super())
}

#[cfg(test)]
mod workshop_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{prelude::*, types::PyTuple};
    use rstest::rstest;

    use super::{Workshop, add_references};
    use crate::{
        ffi::{
            c::prelude::{CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS, CVar, CVarBuilder, cvar_t},
            python::{Plugin, pyshinqlx_setup_fixture::pyshinqlx_setup},
        },
        prelude::*,
    };

    fn workshop_plugin(py: Python<'_>) -> Bound<'_, Workshop> {
        Bound::new(
            py,
            PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Workshop),
        )
        .expect("could not create plugin")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_appends_the_referenced_workshop_items(_pyshinqlx_setup: ()) {
        let references = c"9999,8888";
        let mut raw_references_cvar = CVarBuilder::default()
            .string(references.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "\\mapname\\campgrounds", 1..)
            .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "1234 5678", 1)
            .with_find_cvar(
                |cmd| cmd == "qlx_workshopReferences",
                move |_| CVar::try_from(raw_references_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::eq("1234 5678 9999 8888"),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let result =
                        Workshop::handle_map(&workshop_plugin(py), &PyTuple::empty(py), None);

                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_does_not_add_items_that_are_already_referenced(_pyshinqlx_setup: ()) {
        let references = c"5678, 9999";
        let mut raw_references_cvar = CVarBuilder::default()
            .string(references.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "\\mapname\\campgrounds", 1..)
            .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "1234 5678", 1)
            .with_find_cvar(
                |cmd| cmd == "qlx_workshopReferences",
                move |_| CVar::try_from(raw_references_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::eq("1234 5678 9999"),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let result = add_references(&workshop_plugin(py));

                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_map_without_a_game_does_nothing(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1..)
            .run(|| {
                Python::attach(|py| {
                    let result =
                        Workshop::handle_map(&workshop_plugin(py), &PyTuple::empty(py), None);

                    assert!(result.is_ok());
                });
            });
    }
}
