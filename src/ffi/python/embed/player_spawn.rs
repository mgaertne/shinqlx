use tap::{TapFallible, TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_client_spawn;
#[cfg(not(test))]
use crate::hooks::shinqlx_client_spawn;

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
pub(crate) fn pyshinqlx_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok())
            .tap_some_mut(|game_entity| {
                let _ = game_entity.get_game_client().tap_ok_mut(|game_client| {
                    game_client.spawn();
                });
                shinqlx_client_spawn(game_entity)
            })
            .is_some())
    })
}

#[cfg(test)]
mod player_spawn_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        hooks::mock_hooks::shinqlx_client_spawn_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_spawn_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_player_spawn(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_spawn_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_spawn(py, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_spawn_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_spawn(py, 666);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_spawn_for_existing_game_client(_pyshinqlx_setup: ()) {
        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(1);

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_spawn().times(..=1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_player_spawn(py, 2));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_spawn_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(0);

        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_player_spawn(py, 2));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
