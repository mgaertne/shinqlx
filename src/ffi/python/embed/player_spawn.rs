#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_client_spawn;
#[cfg(not(test))]
use crate::hooks::shinqlx_client_spawn;
use crate::prelude::*;
use crate::MAIN_ENGINE;

use pyo3::exceptions::{PyEnvironmentError, PyValueError};
use pyo3::{pyfunction, PyResult, Python};

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
pub(crate) fn pyshinqlx_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
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
        let mut opt_game_entity = GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok());

        let returned = opt_game_entity.is_some();
        if returned {
            opt_game_entity.iter_mut().for_each(|game_entity| {
                if let Ok(mut game_client) = game_entity.get_game_client() {
                    game_client.spawn();
                }
                shinqlx_client_spawn(game_entity)
            });
        }
        Ok(returned)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_spawn_tests {
    use super::pyshinqlx_player_spawn;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::hooks::mock_hooks::shinqlx_client_spawn_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_spawn_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_player_spawn(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_spawn(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_spawn(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_spawn().times(..=1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(1);

        let result = Python::with_gil(|py| pyshinqlx_player_spawn(py, 2));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn player_spawn_for_entity_with_no_game_client() {
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

        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(0);

        let result = Python::with_gil(|py| pyshinqlx_player_spawn(py, 2));
        assert_eq!(result.expect("result was not OK"), false);
    }
}
