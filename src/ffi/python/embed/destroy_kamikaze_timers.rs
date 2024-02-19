use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
pub(crate) fn pyshinqlx_destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
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

#[cfg(test)]
mod destroy_kamikaze_timers_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn destroy_kamikaze_timers_for_not_in_use_game_entity() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity.expect_get_health().returning(|| 0);
                mock_game_entity
                    .expect_is_kamikaze_timer()
                    .returning(|| true);
                mock_game_entity.expect_free_entity().times(0);
                mock_game_entity
            });

        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity.expect_get_game_client().times(0);
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
                .expect_is_kamikaze_timer()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_destroy_kamikaze_timers);
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn destroy_kamikaze_timers_for_in_use_non_kamikaze_timer() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_get_health().returning(|| 42);
                mock_game_entity.expect_get_game_client().times(0);
                mock_game_entity
                    .expect_is_kamikaze_timer()
                    .returning(|| false);
                mock_game_entity.expect_free_entity().times(0);
                mock_game_entity
            });

        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
                .expect_is_kamikaze_timer()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_destroy_kamikaze_timers);
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn destroy_kamikaze_timers_for_in_use_kamikaze_timer_with_health() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_get_health().returning(|| 42);
                mock_game_entity.expect_get_game_client().times(0);
                mock_game_entity
                    .expect_is_kamikaze_timer()
                    .returning(|| true);
                mock_game_entity.expect_free_entity().times(1);
                mock_game_entity
            });

        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
                .expect_is_kamikaze_timer()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_destroy_kamikaze_timers);
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn destroy_kamikaze_timers_for_in_use_kamikaze_timer_with_no_health_but_no_game_client() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_get_health().returning(|| 0);
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
                    .expect_is_kamikaze_timer()
                    .returning(|| true);
                mock_game_entity.expect_free_entity().times(1);
                mock_game_entity
            });

        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
                .expect_is_kamikaze_timer()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_destroy_kamikaze_timers);
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn destroy_kamikaze_timers_for_in_use_kamikaze_timer_with_no_health_but_game_client() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity.expect_get_health().returning(|| 0);
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_remove_kamikaze_flag().times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
                .expect_is_kamikaze_timer()
                .returning(|| true);
            mock_game_entity.expect_free_entity().times(1);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_destroy_kamikaze_timers);
        assert_eq!(result.expect("result was not OK"), true);
    }
}
