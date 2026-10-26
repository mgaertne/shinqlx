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
