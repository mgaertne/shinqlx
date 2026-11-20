use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyList, PyTuple},
};

use super::create_cvar;
use crate::ffi::python::{
    commands::CommandPriorities,
    game::GameMethods,
    plugin::{CvarType, Plugin, PluginMethods, read_cvar_as},
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

    let references = read_cvar_as(py, "qlx_workshopReferences", CvarType::List)?;
    let present = game.get_workshop_items()?;
    let workshop_items = PyList::new(py, &present)?;
    for reference in references.try_iter()? {
        let reference = reference?;
        let already_present = reference
            .str()?
            .to_string()
            .trim()
            .parse::<u64>()
            .is_ok_and(|id| present.contains(&id));
        if !already_present {
            workshop_items.append(reference)?;
        }
    }
    game.set_workshop_items(&workshop_items)
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Workshop),
    )?;
    plugin.as_super().add_hook(
        "map",
        &plugin.getattr(intern!(py, "handle_map"))?,
        CommandPriorities::PRI_NORMAL as i32,
    )?;
    create_cvar(py, "qlx_workshopReferences", "")?;
    add_references(&plugin)?;

    Ok(plugin.into_any())
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
