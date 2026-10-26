use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyList, PyTuple},
};

use crate::ffi::python::{
    commands::CommandPriorities,
    game::GameMethods,
    plugin::{Plugin, PluginMethods},
};

/// Adds the workshop items of the cvar `qlx_workshopReferences` to every map.
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
        let py = slf.py();
        let Some(game) = slf.as_super().get_game() else {
            return Ok(());
        };
        let game = Bound::new(py, game)?;

        let references = py.get_type::<Plugin>().call_method1(
            intern!(py, "get_cvar"),
            ("qlx_workshopReferences", py.get_type::<PyList>()),
        )?;
        let workshop_items = PyList::new(py, game.get_workshop_items()?)?;
        workshop_items.call_method1(intern!(py, "extend"), (references,))?;
        game.set_workshop_items(&workshop_items)
    }
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
    py.get_type::<Plugin>()
        .call_method1(intern!(py, "set_cvar_once"), ("qlx_workshopReferences", ""))?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod workshop_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{prelude::*, types::PyTuple};
    use rstest::rstest;

    use super::Workshop;
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
