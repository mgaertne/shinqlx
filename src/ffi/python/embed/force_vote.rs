use crate::MAIN_ENGINE;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use pyo3::exceptions::PyEnvironmentError;

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
pub(crate) fn pyshinqlx_force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
    py.allow_threads(|| {
        if CurrentLevel::try_get()
            .ok()
            .and_then(|current_level| current_level.get_vote_time())
            .is_none()
        {
            return Ok(false);
        }

        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| {
                let maxclients = main_engine.get_max_clients();

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
                Ok(true)
            },
        )
    })
}

#[cfg(test)]
mod force_vote_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        Python::with_gil(|py| {
            let result = pyshinqlx_force_vote(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_when_no_vote_is_running(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        let result = Python::with_gil(|py| pyshinqlx_force_vote(py, false));
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[rstest]
    #[case(clientState_t::CS_ZOMBIE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_PRIMED)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_for_non_active_client(#[case] clientstate: clientState_t, _pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        MockEngineBuilder::default().with_max_clients(1).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_for_active_client_with_no_game_client(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

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

        MockEngineBuilder::default().with_max_clients(1).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_for_active_client_forces_vote(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

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

        MockEngineBuilder::default().with_max_clients(1).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_force_vote(py, true));

            assert_eq!(result.expect("result was not OK"), true);
        });
    }
}
