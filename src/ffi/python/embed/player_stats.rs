use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Get some player stats.
#[pyfunction]
#[pyo3(name = "player_stats")]
pub(crate) fn pyshinqlx_player_stats(
    py: Python<'_>,
    client_id: i32,
) -> PyResult<Option<PlayerStats>> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        Ok(GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .map(PlayerStats::from))
    })
}

#[cfg(test)]
mod player_stats_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_stats_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_player_stats(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_stats_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_stats(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_stats_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_stats(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_stats_for_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_score().returning(|| 42);
                mock_game_client.expect_get_kills().returning(|| 7);
                mock_game_client.expect_get_deaths().returning(|| 9);
                mock_game_client
                    .expect_get_damage_dealt()
                    .returning(|| 5000);
                mock_game_client
                    .expect_get_damage_taken()
                    .returning(|| 4200);
                mock_game_client.expect_get_time_on_team().returning(|| 123);
                mock_game_client.expect_get_ping().returning(|| 9);
                Ok(mock_game_client)
            });
            mock_game_entity
        });
        let result = Python::with_gil(|py| pyshinqlx_player_stats(py, 2));

        assert_eq!(
            result
                .expect("result was not OK")
                .expect("result was not Some"),
            PlayerStats {
                score: 42,
                kills: 7,
                deaths: 9,
                damage_dealt: 5000,
                damage_taken: 4200,
                time: 123,
                ping: 9,
            }
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_stats_for_game_entiy_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });
        let result = Python::with_gil(|py| pyshinqlx_player_stats(py, 2));

        assert_eq!(result.expect("result was not OK"), None);
    }
}
