use crate::prelude::*;

use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, PyResult, Python};

/// get a list of entities that target a given entity
#[pyfunction]
#[pyo3(name = "get_targetting_entities")]
pub(crate) fn minqlx_get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
    if entity_id < 0 || entity_id >= MAX_GENTITIES as i32 {
        return Err(PyValueError::new_err(format!(
            "entity_id need to be between 0 and {}.",
            MAX_GENTITIES - 1
        )));
    }

    py.allow_threads(move || {
        GameEntity::try_from(entity_id).map_or_else(
            |_| Ok(vec![]),
            |entity| Ok(entity.get_targetting_entity_ids()),
        )
    })
}
