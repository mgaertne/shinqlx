use tap::TryConv;

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Get information about the player's state in the game.
#[pyfunction]
#[pyo3(name = "player_state")]
pub(crate) fn pyshinqlx_player_state(
    py: Python<'_>,
    client_id: i32,
) -> PyResult<Option<PlayerState>> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok())
            .map(PlayerState::from))
    })
}

#[cfg(test)]
mod player_state_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{
            c::prelude::*,
            python::{prelude::*, pyshinqlx_test_support::default_player_state},
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_player_state(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_state(py, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_state(py, 666);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_without_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_player_state(py, 2));
                    assert_eq!(result.expect("result was not OK"), None);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_transforms_from_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_health(123, 1..)
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_get_position()
                    .returning(|| (1.0, 2.0, 3.0));
                mock_game_client
                    .expect_get_velocity()
                    .returning(|| (4.0, 5.0, 6.0));
                mock_game_client.expect_is_alive().returning(|| true);
                mock_game_client.expect_get_armor().returning(|| 456);
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_NAILGUN);
                mock_game_client
                    .expect_get_weapons()
                    .returning(|| [1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
                mock_game_client
                    .expect_get_ammos()
                    .returning(|| [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
                mock_game_client
                    .expect_get_powerups()
                    .returning(|| [12, 34, 56, 78, 90, 24]);
                mock_game_client
                    .expect_get_holdable()
                    .returning(|| Holdable::Kamikaze.into());
                mock_game_client
                    .expect_get_current_flight_fuel()
                    .returning(|| 12);
                mock_game_client
                    .expect_get_max_flight_fuel()
                    .returning(|| 34);
                mock_game_client.expect_get_flight_thrust().returning(|| 56);
                mock_game_client.expect_get_flight_refuel().returning(|| 78);
                mock_game_client.expect_is_chatting().returning(|| true);
                mock_game_client.expect_is_frozen().returning(|| true);
                Ok(mock_game_client)
            })
            .run(predicate::eq(2), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_player_state(py, 2));
                    assert_eq!(
                        result.expect("result was not OK"),
                        Some(default_player_state())
                    );
                });
            });
    }
}
