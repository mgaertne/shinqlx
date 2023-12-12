use crate::prelude::*;
use crate::MAIN_ENGINE;

use pyo3::exceptions::{PyEnvironmentError, PyValueError};
use pyo3::{pyfunction, PyResult, Python};

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
pub(crate) fn pyshinqlx_set_privileges(
    py: Python<'_>,
    client_id: i32,
    privileges: i32,
) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_privileges(privileges));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_privileges_tests {
    use super::pyshinqlx_set_privileges;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn set_privileges_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_set_privileges(py, 21, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_privileges_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_set_privileges(py, -1, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_privileges_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_set_privileges(py, 666, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(&privileges_t::PRIV_NONE)]
    #[case(&privileges_t::PRIV_MOD)]
    #[case(&privileges_t::PRIV_ADMIN)]
    #[case(&privileges_t::PRIV_ROOT)]
    #[case(&privileges_t::PRIV_BANNED)]
    #[serial]
    fn set_privileges_for_existing_game_client(#[case] privileges: &'static privileges_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_privileges()
                    .with(predicate::eq(*privileges as i32))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_set_privileges(py, 2, *privileges as i32));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn set_privileges_for_entity_with_no_game_client() {
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

        let result =
            Python::with_gil(|py| pyshinqlx_set_privileges(py, 2, privileges_t::PRIV_NONE as i32));
        assert_eq!(result.expect("result was not OK"), false);
    }
}
