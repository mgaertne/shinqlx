use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Get information about the player's state in the game.
#[pyfunction]
#[pyo3(name = "player_state")]
pub(crate) fn pyshinqlx_player_state(
    py: Python<'_>,
    client_id: i32,
) -> PyResult<Option<PlayerState>> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        Ok(GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok())
            .map(PlayerState::from))
    })
}

#[cfg(test)]
mod player_state_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_player_state(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_state(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_player_state(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_for_client_without_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
            });

        let result = Python::with_gil(|py| pyshinqlx_player_state(py, 2));
        assert_eq!(result.expect("result was not OK"), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_state_transforms_from_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
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
                });
                mock_game_entity.expect_get_health().returning(|| 123);
                mock_game_entity
            });

        let result = Python::with_gil(|py| pyshinqlx_player_state(py, 2));
        assert_eq!(
            result.expect("result was not OK"),
            Some(PlayerState {
                is_alive: true,
                position: Vector3(1, 2, 3),
                velocity: Vector3(4, 5, 6),
                health: 123,
                armor: 456,
                noclip: true,
                weapon: weapon_t::WP_NAILGUN.into(),
                weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
                ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
                powerups: Powerups(12, 34, 56, 78, 90, 24),
                holdable: Some("kamikaze".into()),
                flight: Flight(12, 34, 56, 78),
                is_chatting: true,
                is_frozen: true,
            })
        );
    }
}
