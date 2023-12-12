use crate::ffi::c::CurrentLevel;
use crate::prelude::*;
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::{pyfunction, PyResult, Python};

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
pub(crate) fn pyshinqlx_force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
    let vote_time = py.allow_threads(|| {
        CurrentLevel::try_get()
            .ok()
            .and_then(|current_level| current_level.get_vote_time())
    });
    if vote_time.is_none() {
        return Ok(false);
    }

    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        (0..maxclients)
            .filter(|i| {
                Client::try_from(*i)
                    .ok()
                    .filter(|client| client.get_state() == clientState_t::CS_ACTIVE)
                    .is_some()
            })
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_state(pass));
    });

    Ok(true)
}

#[cfg(test)]
#[cfg(not(miri))]
mod force_vote_tests {
    use super::pyshinqlx_force_vote;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
    use crate::ffi::c::current_level::MockTestCurrentLevel;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn force_vote_when_main_engine_not_initialized() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = pyshinqlx_force_vote(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn force_vote_when_no_vote_is_running() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        MAIN_ENGINE.store(None);

        let result = Python::with_gil(|py| pyshinqlx_force_vote(py, false));
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[rstest]
    #[case(clientState_t::CS_ZOMBIE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_PRIMED)]
    #[serial]
    fn force_vote_for_non_active_client(#[case] clientstate: clientState_t) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn force_vote_for_active_client_with_no_game_client() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
            });

        let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn force_vote_for_active_client_forces_vote() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_vote_state()
                        .with(predicate::eq(true))
                        .times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

        assert_eq!(result.expect("result was not OK"), true);
    }
}
