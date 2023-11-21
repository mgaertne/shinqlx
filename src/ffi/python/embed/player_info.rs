use crate::prelude::*;
use crate::MAIN_ENGINE;

use core::sync::atomic::Ordering;
use pyo3::exceptions::{PyEnvironmentError, PyValueError};

use crate::ffi::python::{PlayerInfo, ALLOW_FREE_CLIENT};
use pyo3::prelude::*;

/// Returns a dictionary with information about a plapub(crate) yer by ID.
#[pyfunction(name = "player_info")]
pub(crate) fn pyshinqlx_player_info(
    py: Python<'_>,
    client_id: i32,
) -> PyResult<Option<PlayerInfo>> {
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
        let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let opt_client = Client::try_from(client_id).ok();

        if opt_client
            .filter(|client| {
                client.get_state() == clientState_t::CS_FREE
                    && allowed_free_clients & (1 << client_id as u64) == 0
            })
            .is_some()
        {
            warn!(
                target: "shinqlx",
                "WARNING: get_player_info called for CS_FREE client {}.",
                client_id
            );
            return Ok(None);
        };

        Ok(Some(PlayerInfo::from(client_id)))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_player_info_tests {
    use super::pyshinqlx_player_info;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::ffi::python::player_info::PlayerInfo;
    use crate::ffi::python::ALLOW_FREE_CLIENT;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use core::sync::atomic::Ordering;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_player_info_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_player_info(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = pyshinqlx_player_info(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = pyshinqlx_player_info(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_existing_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_RED);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

        let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
        assert_eq!(
            player_info.expect("result was not OK"),
            Some(PlayerInfo {
                client_id: 2,
                name: "Mocked Player".into(),
                connection_state: clientState_t::CS_ACTIVE as i32,
                userinfo: "asdf".into(),
                steam_id: 1234,
                team: team_t::TEAM_RED as i32,
                privileges: privileges_t::PRIV_NONE as i32
            })
        );
    }

    #[test]
    #[serial]
    fn get_player_info_for_non_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(0, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

        let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
        assert_eq!(player_info.expect("result was not OK"), None);
    }

    #[test]
    #[serial]
    fn get_player_info_for_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(1 << 2, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_RED);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

        let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
        assert_eq!(
            player_info.expect("result was not OK"),
            Some(PlayerInfo {
                client_id: 2,
                name: "Mocked Player".into(),
                connection_state: clientState_t::CS_FREE as i32,
                userinfo: "asdf".into(),
                steam_id: 1234,
                team: team_t::TEAM_RED as i32,
                privileges: privileges_t::PRIV_NONE as i32
            })
        );
    }
}
