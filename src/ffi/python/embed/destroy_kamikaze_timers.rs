use crate::prelude::*;

use pyo3::{pyfunction, PyResult, Python};

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
pub(crate) fn minqlx_destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        let mut in_use_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use())
            .collect();

        in_use_entities
            .iter()
            .filter(|&game_entity| game_entity.get_health() <= 0)
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.remove_kamikaze_flag());

        in_use_entities
            .iter_mut()
            .filter(|game_entity| game_entity.is_kamikaze_timer())
            .for_each(|game_entity| game_entity.free_entity());

        Ok(true)
    })
}
