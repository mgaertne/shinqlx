use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Sets a player's score.
#[pyfunction]
#[pyo3(name = "set_score")]
pub(crate) fn pyshinqlx_set_score(py: Python<'_>, client_id: i32, score: i32) -> PyResult<bool> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_score(score));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_score_tests {
    use super::pyshinqlx_set_score;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};

    #[test]
    #[serial]
    fn set_score_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_set_score(py, 21, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_set_score(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_set_score(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_score()
                    .with(predicate::eq(42))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_set_score(py, 2, 42));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn set_score_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| pyshinqlx_set_score(py, 2, 42));
        assert_eq!(result.expect("result was not OK"), false);
    }
}
