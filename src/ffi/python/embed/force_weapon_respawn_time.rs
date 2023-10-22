use crate::prelude::*;

use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, PyResult, Python};

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "force_weapon_respawn_time")]
pub(crate) fn minqlx_force_weapon_respawn_time(
    py: Python<'_>,
    respawn_time: i32,
) -> PyResult<bool> {
    if respawn_time < 0 {
        return Err(PyValueError::new_err(
            "respawn time needs to be an integer 0 or greater",
        ));
    }

    py.allow_threads(move || {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_respawning_weapon())
            .for_each(|mut game_entity| game_entity.set_respawn_time(respawn_time))
    });

    Ok(true)
}
