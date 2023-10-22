use crate::prelude::*;

use pyo3::{pyfunction, PyResult, Python};

/// Removes all dropped items.
#[pyfunction]
#[pyo3(name = "remove_dropped_items")]
pub(crate) fn minqlx_remove_dropped_items(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.has_flags() && game_entity.is_dropped_item()
            })
            .for_each(|mut game_entity| game_entity.free_entity());
    });

    Ok(true)
}
