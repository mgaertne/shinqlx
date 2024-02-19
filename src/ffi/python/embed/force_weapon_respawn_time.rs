use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use pyo3::exceptions::PyValueError;

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "force_weapon_respawn_time")]
pub(crate) fn pyshinqlx_force_weapon_respawn_time(
    py: Python<'_>,
    respawn_time: i32,
) -> PyResult<bool> {
    py.allow_threads(|| {
        if respawn_time < 0 {
            return Err(PyValueError::new_err(
                "respawn time needs to be an integer 0 or greater",
            ));
        }

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_respawning_weapon())
            .for_each(|mut game_entity| game_entity.set_respawn_time(respawn_time));

        Ok(true)
    })
}

#[cfg(test)]
mod force_weapon_respawn_time_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyValueError;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn force_weapon_respawn_time_with_too_small_respawn_time() {
        Python::with_gil(|py| {
            let result = pyshinqlx_force_weapon_respawn_time(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_weapon_respawn_time_with_non_in_use_weapon() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity
                    .expect_is_respawning_weapon()
                    .returning(|| true);
                mock_game_entity.expect_set_respawn_time().times(0);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_respawning_weapon()
                .returning(|| false);
            mock_game_entity.expect_set_respawn_time().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_force_weapon_respawn_time(py, 123));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_weapon_respawn_time_with_non_respawning_weapon() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_respawning_weapon()
                    .returning(|| false);
                mock_game_entity.expect_set_respawn_time().times(0);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_respawning_weapon()
                .returning(|| false);
            mock_game_entity.expect_set_respawn_time().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_force_weapon_respawn_time(py, 123));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_weapon_respawn_time_sets_respawn_time_on_in_use_respawning_weapons() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity
                .expect_is_respawning_weapon()
                .returning(|| true);
            mock_game_entity
                .expect_set_respawn_time()
                .with(predicate::eq(123))
                .times(1);
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_force_weapon_respawn_time(py, 123));
        assert_eq!(result.expect("result was not OK"), true);
    }
}
