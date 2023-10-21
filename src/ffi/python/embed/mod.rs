use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use crate::commands::cmd_py_command;
#[cfg(test)]
use crate::ffi::python::DUMMY_MAIN_ENGINE as MAIN_ENGINE;
#[cfg(test)]
use crate::hooks::mock_hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
#[cfg(not(test))]
use crate::hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
#[cfg(not(test))]
use crate::MAIN_ENGINE;
use core::sync::atomic::Ordering;

#[cfg(not(test))]
use crate::ffi::c::current_level::CurrentLevel;
#[cfg(test)]
use crate::ffi::c::current_level::MockTestCurrentLevel as CurrentLevel;
use crate::ffi::c::game_item::GameItem;
use crate::ffi::python::{
    ALLOW_FREE_CLIENT, CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, CUSTOM_COMMAND_HANDLER,
    DAMAGE_HANDLER, FRAME_HANDLER, KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER,
    NEW_GAME_HANDLER, PLAYER_CONNECT_HANDLER, PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER,
    PLAYER_SPAWN_HANDLER, RCON_HANDLER, SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
};
use crate::quake_live_engine::{
    AddCommand, ComPrintf, ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SendServerCommand,
    SetCVarForced, SetCVarLimit,
};
use pyo3::exceptions::{PyEnvironmentError, PyTypeError, PyValueError};
use pyo3::prelude::*;

use super::{Flight, Holdable, PlayerInfo, PlayerState, PlayerStats, Powerups, Vector3, Weapons};

/// Returns a dictionary with information about a plapub(crate) yer by ID.
#[pyfunction(name = "player_info")]
pub(crate) fn get_player_info(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerInfo>> {
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

    py.allow_threads(move || {
        let Ok(client) = Client::try_from(client_id) else {
            return Ok(Some(PlayerInfo::from(client_id)));
        };

        let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        if allowed_free_clients & (1 << client_id as u64) == 0
            && client.get_state() == clientState_t::CS_FREE
        {
            warn!(
                target: "shinqlx",
                "WARNING: get_player_info called for CS_FREE client {}.",
                client_id
            );
            return Ok(None);
        }

        Ok(Some(PlayerInfo::from(client_id)))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_player_info_tests {
    use super::get_player_info;
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
            let result = get_player_info(py, 0);
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
            let result = get_player_info(py, -1);
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
            let result = get_player_info(py, 42);
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

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(
            player_info,
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

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(player_info, None);
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

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(
            player_info,
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

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
pub(crate) fn get_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    py.allow_threads(move || {
        let result: Vec<Option<PlayerInfo>> = (0..maxclients)
            .filter_map(|client_id| {
                Client::try_from(client_id).map_or_else(
                    |_| None,
                    |client| match client.get_state() {
                        clientState_t::CS_FREE => None,
                        _ => Some(Some(PlayerInfo::from(client_id))),
                    },
                )
            })
            .collect();

        Ok(result)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_players_info_tests {
    use super::get_players_info;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_players_info_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_players_info(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }
}

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
pub(crate) fn get_userinfo(py: Python<'_>, client_id: i32) -> PyResult<Option<String>> {
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

    py.allow_threads(move || {
        let opt_client = Client::try_from(client_id).ok().filter(|client| {
            let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
            client.get_state() != clientState_t::CS_FREE
                || allowed_free_clients & (1 << client_id as u64) != 0
        });
        Ok(opt_client.map(|client| client.get_user_info()))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_userinfo_tests {
    use super::MAIN_ENGINE;
    use super::{get_userinfo, ALLOW_FREE_CLIENT};
    use crate::ffi::c::client::MockClient;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_userinfo_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_userinfo(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_userinfo(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_userinfo(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_existing_client() {
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
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn get_userinfo_for_non_allowed_free_client() {
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
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, None);
    }

    #[test]
    #[serial]
    fn get_userinfo_for_allowed_free_client() {
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
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }
}

/// Sends a server command to either one specific client or all the clients.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
pub(crate) fn send_server_command(
    py: Python<'_>,
    client_id: Option<i32>,
    cmd: &str,
) -> PyResult<bool> {
    match client_id {
        None => {
            #[allow(clippy::unnecessary_to_owned)]
            shinqlx_send_server_command(None, cmd.to_string());
            Ok(true)
        }
        Some(actual_client_id) => {
            let maxclients = py.allow_threads(|| {
                let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                    return Err(PyEnvironmentError::new_err(
                        "main quake live engine not set",
                    ));
                };

                Ok(main_engine.get_max_clients())
            })?;

            if !(0..maxclients).contains(&actual_client_id) {
                return Err(PyValueError::new_err(format!(
                    "client_id needs to be a number from 0 to {}, or None.",
                    maxclients - 1
                )));
            }

            let opt_client = Client::try_from(actual_client_id)
                .ok()
                .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
            let returned = opt_client.is_some();
            #[allow(clippy::unnecessary_to_owned)]
            if returned {
                shinqlx_send_server_command(opt_client, cmd.to_string());
            }
            Ok(returned)
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod send_server_command_tests {
    use super::send_server_command;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn send_server_command_with_no_client_id() {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "asdf")
            .times(1);
        let result = Python::with_gil(|py| send_server_command(py, None, "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn send_server_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(0), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(-1), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(42), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn send_server_command_for_active_client() {
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
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "asdf")
            .times(1);

        let result = Python::with_gil(|py| send_server_command(py, Some(2), "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn send_server_command_for_non_active_free_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        let result = Python::with_gil(|py| send_server_command(py, Some(2), "asdf")).unwrap();
        assert_eq!(result, false);
    }
}

/// Tells the server to process a command from a specific client.
#[pyfunction]
#[pyo3(name = "client_command")]
pub(crate) fn client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<bool> {
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
            "client_id needs to be a number from 0 to {}, or None.",
            maxclients - 1
        )));
    }

    let opt_client = Client::try_from(client_id).ok().filter(|client| {
        ![clientState_t::CS_FREE, clientState_t::CS_ZOMBIE].contains(&client.get_state())
    });
    let returned = opt_client.is_some();
    if returned {
        shinqlx_execute_client_command(opt_client, cmd.to_string(), true);
    }
    Ok(returned)
}

#[cfg(test)]
#[cfg(not(miri))]
mod client_command_tests {
    use super::client_command;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_execute_client_command_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::*;

    #[test]
    #[serial]
    fn client_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = client_command(py, 0, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn client_command_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = client_command(py, -1, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn client_command_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = client_command(py, 42, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_ACTIVE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[serial]
    fn send_server_command_for_active_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd, &client_ok| client.is_some() && cmd == "asdf" && client_ok)
            .times(1);

        let result = Python::with_gil(|py| client_command(py, 2, "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn send_server_command_for_non_active_free_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        let result = Python::with_gil(|py| client_command(py, 2, "asdf")).unwrap();
        assert_eq!(result, false);
    }
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
pub(crate) fn console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.execute_console_command(cmd);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod console_command_tests {
    use super::console_command;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = console_command(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn console_command_with_main_engine_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("asdf"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
pub(crate) fn get_cvar(py: Python<'_>, cvar: &str) -> PyResult<Option<String>> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        match main_engine.find_cvar(cvar) {
            None => Ok(None),
            Some(cvar_result) => Ok(Some(cvar_result.get_string())),
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_cvar_tests {
    use super::get_cvar;
    use super::MAIN_ENGINE;
    use crate::ffi::c::cvar::CVar;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use alloc::ffi::CString;
    use core::ffi::c_char;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_cvar(py, "sv_maxclients");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_cvar_when_cvar_not_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("asdf"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_cvar(py, "asdf")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn get_cvar_when_cvar_is_found() {
        let cvar_string = CString::new("16").unwrap();
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_cvar(py, "sv_maxclients")).unwrap();
        assert_eq!(result, Some("16".into()));
    }
}

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
pub(crate) fn set_cvar(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    flags: Option<i32>,
) -> PyResult<bool> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        match main_engine.find_cvar(cvar) {
            None => {
                main_engine.get_cvar(cvar, value, flags);
                Ok(true)
            }
            Some(_) => {
                main_engine.set_cvar_forced(
                    cvar,
                    value,
                    flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
                );
                Ok(false)
            }
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_tests {
    use super::set_cvar;
    use super::MAIN_ENGINE;
    use crate::ffi::c::cvar::CVar;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_cvar(py, "sv_maxclients", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_for_not_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        mock_engine
            .expect_get_cvar()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_cvar_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine
            .expect_set_cvar_forced()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(false),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
pub(crate) fn set_cvar_limit(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    min: &str,
    max: &str,
    flags: Option<i32>,
) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.set_cvar_limit(cvar, value, min, max, flags);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_limit_tests {
    use super::set_cvar_limit;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_cvar_limit(py, "sv_maxclients", "64", "1", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_limit_forwards_parameters_to_main_engine_call() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_cvar_limit()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq("1"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar_limit(
                py,
                "sv_maxclients",
                "64",
                "1",
                "64",
                Some(cvar_flags::CVAR_CHEAT as i32),
            )
        });
        assert!(result.is_ok());
    }
}

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
pub(crate) fn kick(py: Python<'_>, client_id: i32, reason: Option<&str>) -> PyResult<()> {
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

    py.allow_threads(move || {
        let mut opt_client = Client::try_from(client_id)
            .ok()
            .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
        let reason_str = reason
            .filter(|rsn| !rsn.is_empty())
            .unwrap_or("was kicked.");
        #[allow(clippy::unnecessary_to_owned)]
        opt_client
            .iter_mut()
            .for_each(|client| shinqlx_drop_client(client, reason_str.to_string()));
        if opt_client.is_some() {
            Ok(())
        } else {
            Err(PyValueError::new_err(
                "client_id must be the ID of an active player.",
            ))
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod kick_tests {
    use super::kick;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_drop_client_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn kick_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = kick(py, 0, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = kick(py, -1, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = kick(py, 42, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn kick_with_client_id_for_non_active_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        Python::with_gil(|py| {
            let result = kick(py, 2, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_without_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, None));
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_with_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "please go away!")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, Some("please go away!")));
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_with_empty_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, Some("")));
        assert!(result.is_ok());
    }
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
pub(crate) fn console_print(py: Python<'_>, text: &str) {
    py.allow_threads(move || {
        let formatted_string = format!("{}\n", text);
        shinqlx_com_printf(formatted_string.as_str());
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod console_print_tests {
    use super::console_print as py_console_print;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn console_print() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx.expect().with(predicate::eq("asdf\n"));

        Python::with_gil(|py| {
            py_console_print(py, "asdf");
        });
    }
}

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
pub(crate) fn get_configstring(py: Python<'_>, config_id: u32) -> PyResult<String> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_configstring(config_id as u16))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_configstring_tests {
    use super::get_configstring;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_configstring_for_too_large_configstring_id() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_configstring(py, MAX_CONFIGSTRINGS + 1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_configstring_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_configstring(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_configstring_forwards_call_to_engine() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(666))
            .returning(|_| "asdf".into())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_configstring(py, 666)).unwrap();
        assert_eq!(result, "asdf");
    }
}

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
pub(crate) fn set_configstring(py: Python<'_>, config_id: u32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(config_id, value.to_string());

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_configstring_tests {
    use super::set_configstring;
    use crate::hooks::mock_hooks::shinqlx_set_configstring_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_configstring_with_index_out_of_bounds() {
        Python::with_gil(|py| {
            let result = set_configstring(py, 2048, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_configstring_with_proper_index() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(666), predicate::eq("asdf".to_string()))
            .times(1);

        let result = Python::with_gil(|py| set_configstring(py, 666, "asdf"));
        assert!(result.is_ok());
    }
}

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
pub(crate) fn force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
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

    py.allow_threads(move || {
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
    use super::force_vote;
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
            let result = force_vote(py, true);
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

        let result = Python::with_gil(|py| force_vote(py, false)).unwrap();
        assert_eq!(result, false);
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

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
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

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
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

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
    }
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
pub(crate) fn add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        #[allow(clippy::unnecessary_to_owned)]
        main_engine.add_command(command.to_string(), cmd_py_command);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod add_console_command_tests {
    use super::add_console_command;
    use super::MAIN_ENGINE;
    use crate::commands::cmd_py_command;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn add_console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = add_console_command(py, "slap");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn add_console_command_adds_py_command_to_main_engine() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_add_command()
            .withf(|cmd, &func| cmd == "asdf" && func as usize == cmd_py_command as usize)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| add_console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
pub(crate) fn register_handler(
    py: Python<'_>,
    event: &str,
    handler: Option<Py<PyAny>>,
) -> PyResult<()> {
    if handler
        .as_ref()
        .is_some_and(|handler_function| !handler_function.as_ref(py).is_callable())
    {
        return Err(PyTypeError::new_err("The handler must be callable."));
    }

    let handler_lock = match event {
        "client_command" => &CLIENT_COMMAND_HANDLER,
        "server_command" => &SERVER_COMMAND_HANDLER,
        "frame" => &FRAME_HANDLER,
        "player_connect" => &PLAYER_CONNECT_HANDLER,
        "player_loaded" => &PLAYER_LOADED_HANDLER,
        "player_disconnect" => &PLAYER_DISCONNECT_HANDLER,
        "custom_command" => &CUSTOM_COMMAND_HANDLER,
        "new_game" => &NEW_GAME_HANDLER,
        "set_configstring" => &SET_CONFIGSTRING_HANDLER,
        "rcon" => &RCON_HANDLER,
        "console_print" => &CONSOLE_PRINT_HANDLER,
        "player_spawn" => &PLAYER_SPAWN_HANDLER,
        "kamikaze_use" => &KAMIKAZE_USE_HANDLER,
        "kamikaze_explode" => &KAMIKAZE_EXPLODE_HANDLER,
        "damage" => &DAMAGE_HANDLER,
        _ => return Err(PyValueError::new_err("Unsupported event.")),
    };

    py.allow_threads(move || {
        handler_lock.store(handler.map(|handler_func| handler_func.into()));
        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod register_handler_tests {
    use super::register_handler;
    use super::{
        CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, CUSTOM_COMMAND_HANDLER, DAMAGE_HANDLER,
        FRAME_HANDLER, KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER, NEW_GAME_HANDLER,
        PLAYER_CONNECT_HANDLER, PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER,
        PLAYER_SPAWN_HANDLER, RCON_HANDLER, SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
    };
    use crate::prelude::*;
    use once_cell::sync::Lazy;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;
    use swap_arc::SwapArcOption;

    #[rstest]
    #[case("client_command", &CLIENT_COMMAND_HANDLER)]
    #[case("server_command", &SERVER_COMMAND_HANDLER)]
    #[case("frame", &FRAME_HANDLER)]
    #[case("player_connect", &PLAYER_CONNECT_HANDLER)]
    #[case("player_loaded", &PLAYER_LOADED_HANDLER)]
    #[case("player_disconnect", &PLAYER_DISCONNECT_HANDLER)]
    #[case("custom_command", &CUSTOM_COMMAND_HANDLER)]
    #[case("new_game", &NEW_GAME_HANDLER)]
    #[case("set_configstring", &SET_CONFIGSTRING_HANDLER)]
    #[case("rcon", &RCON_HANDLER)]
    #[case("console_print", &CONSOLE_PRINT_HANDLER)]
    #[case("player_spawn", &PLAYER_SPAWN_HANDLER)]
    #[case("kamikaze_use", &KAMIKAZE_USE_HANDLER)]
    #[case("kamikaze_explode", &KAMIKAZE_EXPLODE_HANDLER)]
    #[case("damage", &DAMAGE_HANDLER)]
    #[serial]
    fn register_handler_setting_handler_to_none(
        #[case] event: &str,
        #[case] handler: &Lazy<SwapArcOption<Py<PyAny>>>,
    ) {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        handler.store(Some(py_handler.into()));

        let result = Python::with_gil(|py| register_handler(py, event, None));
        assert!(result.is_ok());

        let stored_handler = handler.load();
        assert!(stored_handler.is_none());
    }

    #[rstest]
    #[case("client_command", &CLIENT_COMMAND_HANDLER)]
    #[case("server_command", &SERVER_COMMAND_HANDLER)]
    #[case("frame", &FRAME_HANDLER)]
    #[case("player_connect", &PLAYER_CONNECT_HANDLER)]
    #[case("player_loaded", &PLAYER_LOADED_HANDLER)]
    #[case("player_disconnect", &PLAYER_DISCONNECT_HANDLER)]
    #[case("custom_command", &CUSTOM_COMMAND_HANDLER)]
    #[case("new_game", &NEW_GAME_HANDLER)]
    #[case("set_configstring", &SET_CONFIGSTRING_HANDLER)]
    #[case("rcon", &RCON_HANDLER)]
    #[case("console_print", &CONSOLE_PRINT_HANDLER)]
    #[case("player_spawn", &PLAYER_SPAWN_HANDLER)]
    #[case("kamikaze_use", &KAMIKAZE_USE_HANDLER)]
    #[case("kamikaze_explode", &KAMIKAZE_EXPLODE_HANDLER)]
    #[case("damage", &DAMAGE_HANDLER)]
    #[serial]
    fn register_handler_setting_handler_to_some_handler(
        #[case] event: &str,
        #[case] handler: &Lazy<SwapArcOption<Py<PyAny>>>,
    ) {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        handler.store(None);

        let result = Python::with_gil(|py| register_handler(py, event, Some(py_handler)));
        assert!(result.is_ok());

        let stored_handler = handler.load();
        assert!(stored_handler.is_some());
    }

    #[test]
    #[serial]
    fn register_handler_for_some_unknown_event() {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        Python::with_gil(|py| {
            let result = register_handler(py, "unknown_event", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn register_handler_for_uncallable_handler() {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
handler = True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        Python::with_gil(|py| {
            let result = register_handler(py, "client_command", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}

/// Get information about the player's state in the game.
#[pyfunction]
#[pyo3(name = "player_state")]
pub(crate) fn get_player_state(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerState>> {
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

    py.allow_threads(move || {
        Ok(GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok())
            .map(PlayerState::from))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_state_tests {
    use super::{get_player_state, Holdable, PlayerState, Vector3};
    use super::{Flight, Powerups, Weapons, MAIN_ENGINE};
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_state_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_player_state(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_state_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = get_player_state(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_state_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = get_player_state(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
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

        let result = Python::with_gil(|py| get_player_state(py, 2)).unwrap();
        assert_eq!(result, None);
    }

    #[test]
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

        let result = Python::with_gil(|py| get_player_state(py, 2)).unwrap();
        assert_eq!(
            result,
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

/// Get some player stats.
#[pyfunction]
#[pyo3(name = "player_stats")]
pub(crate) fn get_player_stats(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerStats>> {
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

    py.allow_threads(move || {
        Ok(GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .map(PlayerStats::from))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_stats_tests {
    use super::MAIN_ENGINE;
    use super::{get_player_stats, PlayerStats};
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_stats_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_player_stats(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_stats_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = get_player_stats(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_stats_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = get_player_stats(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
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
        let result = Python::with_gil(|py| get_player_stats(py, 2)).unwrap();

        assert!(result.is_some_and(|pstats| pstats
            == PlayerStats {
                score: 42,
                kills: 7,
                deaths: 9,
                damage_dealt: 5000,
                damage_taken: 4200,
                time: 123,
                ping: 9,
            }));
    }

    #[test]
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
        let result = Python::with_gil(|py| get_player_stats(py, 2)).unwrap();

        assert_eq!(result, None);
    }
}

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
pub(crate) fn set_position(py: Python<'_>, client_id: i32, position: Vector3) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_position((position.0 as f32, position.1 as f32, position.2 as f32))
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_position_tests {
    use super::set_position;
    use super::Vector3;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_position_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_position(py, 21, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_position(py, -1, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_position(py, 666, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_position()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_position(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_position_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_position(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
pub(crate) fn set_velocity(py: Python<'_>, client_id: i32, velocity: Vector3) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_velocity((velocity.0 as f32, velocity.1 as f32, velocity.2 as f32))
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_velocity_tests {
    use super::set_velocity;
    use super::Vector3;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_velocity_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_velocity(py, 21, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_velocity(py, -1, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_velocity(py, 666, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_velocity()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_velocity(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_velocity_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_velocity(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
pub(crate) fn noclip(py: Python<'_>, client_id: i32, activate: bool) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .filter(|game_client| game_client.get_noclip() != activate);
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_noclip(activate);
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod noclip_tests {
    use super::noclip;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn noclip_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = noclip(py, 21, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = noclip(py, -1, false);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = noclip(py, 666, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| noclip(py, 2, true)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_noclip_already_set_properly() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client.expect_set_noclip::<bool>().times(0);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| noclip(py, 2, true)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_change_applied() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client
                    .expect_set_noclip::<bool>()
                    .with(predicate::eq(false))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| noclip(py, 2, false)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's health.
#[pyfunction]
#[pyo3(name = "set_health")]
pub(crate) fn set_health(py: Python<'_>, client_id: i32, health: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_entity = GameEntity::try_from(client_id).ok();
        opt_game_entity
            .iter_mut()
            .for_each(|game_entity| game_entity.set_health(health));
        Ok(opt_game_entity.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_health_tests {
    use super::set_health;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_health_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_health(py, 21, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_health(py, -1, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_health(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_set_health()
                .with(predicate::eq(666))
                .times(1);
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_health(py, 2, 666)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's armor.
#[pyfunction]
#[pyo3(name = "set_armor")]
pub(crate) fn set_armor(py: Python<'_>, client_id: i32, armor: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_armor(armor));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_armor_tests {
    use super::set_armor;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_armor_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_armor(py, 21, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_armor(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_armor(py, 666, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_armor()
                    .with(predicate::eq(456))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_armor(py, 2, 456)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_armor_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_armor(py, 2, 123)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
pub(crate) fn set_weapons(py: Python<'_>, client_id: i32, weapons: Weapons) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_weapons(weapons.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_weapons_tests {
    use super::set_weapons;
    use super::Weapons;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_weapons_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_weapons(py, 21, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapons(py, -1, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapons(
                py,
                666,
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapons()
                    .with(predicate::eq([1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_weapons(py, 2, Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1))
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_weapons_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| {
            set_weapons(py, 2, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
pub(crate) fn set_weapon(py: Python<'_>, client_id: i32, weapon: i32) -> PyResult<bool> {
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

    if !(0..16).contains(&weapon) {
        return Err(PyValueError::new_err(
            "Weapon must be a number from 0 to 15.",
        ));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_weapon(weapon));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_weapon_tests {
    use super::set_weapon;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_weapon_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_weapon(py, 21, weapon_t::WP_ROCKET_LAUNCHER.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, -1, weapon_t::WP_GRAPPLING_HOOK.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 666, weapon_t::WP_PROX_LAUNCHER.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_weapon_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 2, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_weapon_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 2, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapon()
                    .with(predicate::eq(weapon_t::WP_BFG as i32))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_weapon(py, 2, weapon_t::WP_BFG.into())).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_weapon_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_weapon(py, 2, weapon_t::WP_HMG.into())).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's ammo.
#[pyfunction]
#[pyo3(name = "set_ammo")]
pub(crate) fn set_ammo(py: Python<'_>, client_id: i32, ammos: Weapons) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_ammos(ammos.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_ammo_tests {
    use super::set_ammo;
    use super::Weapons;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_ammo_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_ammo(py, 21, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_ammo(py, -1, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_ammo(
                py,
                666,
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_ammos()
                    .with(predicate::eq([
                        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
                    ]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_ammo(
                py,
                2,
                Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            )
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_ammo_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| {
            set_ammo(py, 2, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
pub(crate) fn set_powerups(py: Python<'_>, client_id: i32, powerups: Powerups) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_powerups(powerups.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_powerups_tests {
    use super::set_powerups;
    use super::Powerups;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_powerups_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_powerups(py, 21, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_powerups(py, -1, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_powerups(py, 666, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_powerups()
                    .with(predicate::eq([1, 2, 3, 4, 5, 6]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| set_powerups(py, 2, Powerups(1, 2, 3, 4, 5, 6))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_powerups_for_entity_with_no_game_client() {
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
            Python::with_gil(|py| set_powerups(py, 2, Powerups(0, 0, 0, 0, 0, 0))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
pub(crate) fn set_holdable(py: Python<'_>, client_id: i32, holdable: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        let ql_holdable = Holdable::from(holdable);
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_holdable(ql_holdable));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_holdable_tests {
    use super::MAIN_ENGINE;
    use super::{set_holdable, Holdable};
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_holdable_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_holdable(py, 21, Holdable::Kamikaze as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_holdable(py, -1, Holdable::Invulnerability as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_holdable(py, 666, Holdable::Teleporter as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_holdable()
                    .with(predicate::eq(Holdable::Kamikaze))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_holdable(py, 2, Holdable::Kamikaze as i32)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_holdable_for_entity_with_no_game_client() {
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
            Python::with_gil(|py| set_holdable(py, 2, Holdable::Invulnerability as i32)).unwrap();
        assert_eq!(result, false);
    }
}

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
pub(crate) fn drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
        GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .iter_mut()
            .for_each(|game_client| game_client.remove_kamikaze_flag());
        let mut opt_game_entity_with_holdable =
            GameEntity::try_from(client_id).ok().filter(|game_entity| {
                game_entity
                    .get_game_client()
                    .ok()
                    .filter(|game_client| {
                        Holdable::from(game_client.get_holdable()) != Holdable::None
                    })
                    .is_some()
            });
        opt_game_entity_with_holdable
            .iter_mut()
            .for_each(|game_entity| game_entity.drop_holdable());
        Ok(opt_game_entity_with_holdable.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod drop_holdable_tests {
    use super::MAIN_ENGINE;
    use super::{drop_holdable, Holdable};
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::Sequence;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn drop_holdable_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = drop_holdable(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = drop_holdable(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = drop_holdable(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn drop_holdable_for_entity_with_no_holdable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_get_holdable().returning(|| 0);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, false);
    }

    #[rstest]
    #[case(&Holdable::Teleporter)]
    #[case(&Holdable::MedKit)]
    #[case(&Holdable::Kamikaze)]
    #[case(&Holdable::Portal)]
    #[case(&Holdable::Invulnerability)]
    #[case(&Holdable::Flight)]
    #[serial]
    fn drop_holdable_for_entity_with_holdable_dropped(#[case] holdable: &'static Holdable) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_get_holdable()
                        .returning(|| *holdable as i32);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(1);
                mock_game_entity
            });

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
pub(crate) fn set_flight(py: Python<'_>, client_id: i32, flight: Flight) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_flight::<[i32; 4]>(flight.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_flight_tests {
    use super::set_flight;
    use super::Flight;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_flight_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_flight(py, 21, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_flight(py, -1, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_flight(py, 666, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_flight::<[i32; 4]>()
                    .with(predicate::eq([12, 34, 56, 78]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_flight(py, 2, Flight(12, 34, 56, 78))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_flight_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_flight(py, 2, Flight(12, 34, 56, 78))).unwrap();
        assert_eq!(result, false);
    }
}

/// Makes player invulnerable for limited time.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
pub(crate) fn set_invulnerability(py: Python<'_>, client_id: i32, time: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_invulnerability(time));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_invulnerability_tests {
    use super::set_invulnerability;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_invulnerability_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_invulnerability(py, 21, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_invulnerability(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_invulnerability(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_invulnerability()
                    .with(predicate::eq(42))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_invulnerability(py, 2, 42)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_entity_with_no_game_client() {
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

        let result = Python::with_gil(|py| set_invulnerability(py, 2, 42)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's score.
#[pyfunction]
#[pyo3(name = "set_score")]
pub(crate) fn set_score(py: Python<'_>, client_id: i32, score: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
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
    use super::set_score;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_score_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_score(py, 21, 42);
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
            let result = set_score(py, -1, 42);
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
            let result = set_score(py, 666, 42);
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

        let result = Python::with_gil(|py| set_score(py, 2, 42)).unwrap();
        assert_eq!(result, true);
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

        let result = Python::with_gil(|py| set_score(py, 2, 42)).unwrap();
        assert_eq!(result, false);
    }
}

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
pub(crate) fn callvote(py: Python<'_>, vote: &str, vote_disp: &str, vote_time: Option<i32>) {
    py.allow_threads(move || {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.callvote(vote, vote_disp, vote_time));
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod callvote_tests {
    use super::callvote;
    use crate::ffi::c::current_level::MockTestCurrentLevel;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn callvote_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| callvote(py, "map thunderstruck", "map thunderstruck", None));
    }

    #[test]
    #[serial]
    fn callvote_with_current_level_calls_vote() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_callvote()
                .with(
                    predicate::eq("map theatreofpain"),
                    predicate::eq("map Theatre of Pain"),
                    predicate::eq(Some(10)),
                )
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| callvote(py, "map theatreofpain", "map Theatre of Pain", Some(10)));
    }
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
pub(crate) fn allow_single_player(py: Python<'_>, allow: bool) {
    py.allow_threads(move || {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.set_training_map(allow))
    });
}

#[cfg(test)]
#[cfg(not(miri))]
mod allow_single_player_tests {
    use super::allow_single_player;
    use crate::ffi::c::current_level::MockTestCurrentLevel;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn allow_single_player_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| allow_single_player(py, true));
    }

    #[test]
    #[serial]
    fn allow_single_player_sets_training_map() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_set_training_map()
                .with(predicate::eq(true))
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| allow_single_player(py, true));
    }
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
pub(crate) fn player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
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
    use super::player_spawn;
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
            let result = player_spawn(py, 21);
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
            let result = player_spawn(py, -1);
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
            let result = player_spawn(py, 666);
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

        let result = Python::with_gil(|py| player_spawn(py, 2)).unwrap();
        assert_eq!(result, true);
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

        let result = Python::with_gil(|py| player_spawn(py, 2)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
pub(crate) fn set_privileges(py: Python<'_>, client_id: i32, privileges: i32) -> PyResult<bool> {
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

    py.allow_threads(move || {
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
    use super::set_privileges;
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
            let result = set_privileges(py, 21, privileges_t::PRIV_MOD as i32);
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
            let result = set_privileges(py, -1, privileges_t::PRIV_MOD as i32);
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
            let result = set_privileges(py, 666, privileges_t::PRIV_MOD as i32);
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

        let result = Python::with_gil(|py| set_privileges(py, 2, *privileges as i32)).unwrap();
        assert_eq!(result, true);
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
            Python::with_gil(|py| set_privileges(py, 2, privileges_t::PRIV_NONE as i32)).unwrap();
        assert_eq!(result, false);
    }
}

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
pub(crate) fn destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        let mut in_use_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use())
            .collect();

        in_use_entities
            .iter()
            .filter(|&game_entity| game_entity.get_health() <= 0)
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.remove_kamikaze_flag());

        in_use_entities
            .iter_mut()
            .filter(|game_entity| game_entity.is_kamikaze_timer())
            .for_each(|game_entity| game_entity.free_entity());

        Ok(true)
    })
}

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
#[pyo3(signature = (item_id, x, y, z))]
pub(crate) fn spawn_item(py: Python<'_>, item_id: i32, x: i32, y: i32, z: i32) -> PyResult<bool> {
    let max_items: i32 = GameItem::get_num_items();
    if !(1..max_items).contains(&item_id) {
        return Err(PyValueError::new_err(format!(
            "item_id needs to be a number from 1 to {}.",
            max_items - 1
        )));
    }

    py.allow_threads(move || {
        let mut gitem = GameItem::try_from(item_id).unwrap();
        gitem.spawn((x, y, z));
    });

    Ok(true)
}

/// Removes all dropped items.
#[pyfunction]
#[pyo3(name = "remove_dropped_items")]
pub(crate) fn remove_dropped_items(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.has_flags() && game_entity.is_dropped_item()
            })
            .for_each(|mut game_entity| game_entity.free_entity());
    });

    Ok(true)
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "slay_with_mod")]
pub(crate) fn slay_with_mod(py: Python<'_>, client_id: i32, mean_of_death: i32) -> PyResult<bool> {
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

    let Ok(means_of_death): Result<meansOfDeath_t, _> = mean_of_death.try_into() else {
        return Err(PyValueError::new_err(
            "means of death needs to be a valid enum value.",
        ));
    };

    py.allow_threads(move || {
        let mut opt_game_entity = GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok());
        opt_game_entity.iter_mut().for_each(|game_entity| {
            if game_entity.get_health() > 0 {
                game_entity.slay_with_mod(means_of_death);
            }
        });
        Ok(opt_game_entity.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod slay_with_mod_tests {
    use super::slay_with_mod;
    use super::MAIN_ENGINE;
    use crate::ffi::c::game_client::MockGameClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn slay_with_mod_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = slay_with_mod(py, 21, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, -1, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, 666, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_invalid_means_of_death() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, 2, 12345);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_remaining_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_slay_with_mod()
                .with(predicate::eq(meansOfDeath_t::MOD_PROXIMITY_MINE))
                .times(1);
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32))
                .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_no_remaining_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 0);
            mock_game_entity.expect_slay_with_mod().times(0);
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32))
                .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_entity_with_no_game_client() {
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
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_CRUSH as i32)).unwrap();
        assert_eq!(result, false);
    }
}

fn determine_item_id(item: &PyAny) -> PyResult<i32> {
    if let Ok(item_id) = item.extract::<i32>() {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            return Err(PyValueError::new_err(format!(
                "item_id needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }
        return Ok(item_id);
    }

    let Ok(item_classname) = item.extract::<String>() else {
        return Err(PyValueError::new_err(
            "item needs to be type of int or string.",
        ));
    };

    (1..GameItem::get_num_items())
        .filter(|i| {
            let game_item = GameItem::try_from(*i);
            game_item.is_ok() && game_item.unwrap().get_classname() == item_classname
        })
        .take(1)
        .next()
        .ok_or(PyValueError::new_err(format!(
            "invalid item classname: {}",
            item_classname
        )))
}

/// Replaces target entity's item with specified one.
#[pyfunction]
#[pyo3(name = "replace_items")]
#[pyo3(signature = (item1, item2))]
pub(crate) fn replace_items(py: Python<'_>, item1: Py<PyAny>, item2: Py<PyAny>) -> PyResult<bool> {
    let item2_id = determine_item_id(item2.as_ref(py))?;
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed

    if let Ok(item1_id) = item1.extract::<i32>(py) {
        // replacing item by entity_id

        // entity_id checking
        if item1_id < 0 || item1_id >= MAX_GENTITIES as i32 {
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
            )));
        }

        return py.allow_threads(move || {
            match GameEntity::try_from(item1_id) {
                Err(_) => return Err(PyValueError::new_err("game entity does not exist")),
                Ok(game_entity) => {
                    if !game_entity.in_use() {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not in use.",
                            item1_id
                        )));
                    }
                    if !game_entity.is_game_item(entityType_t::ET_ITEM) {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not item. Cannot replace it",
                            item1_id
                        )));
                    }
                    let mut mut_game_entity = game_entity;
                    mut_game_entity.replace_item(item2_id);
                }
            }
            Ok(true)
        });
    }

    if let Ok(item1_classname) = item1.extract::<String>(py) {
        let item_found = py.allow_threads(move || {
            let matching_item1_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
                .filter_map(|i| GameEntity::try_from(i as i32).ok())
                .filter(|game_entity| {
                    game_entity.in_use()
                        && game_entity.is_game_item(entityType_t::ET_ITEM)
                        && game_entity.get_classname() == item1_classname
                })
                .collect();
            let item_found = !matching_item1_entities.is_empty();
            matching_item1_entities
                .into_iter()
                .for_each(|mut game_entity| game_entity.replace_item(item2_id));
            item_found
        });
        return Ok(item_found);
    }

    Err(PyValueError::new_err(
        "entity needs to be type of int or string.",
    ))
}

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
pub(crate) fn dev_print_items(py: Python<'_>) -> PyResult<()> {
    let formatted_items: Vec<String> = py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
            })
            .map(|game_entity| {
                format!(
                    "{} {}",
                    game_entity.get_entity_id(),
                    game_entity.get_classname()
                )
            })
            .collect()
    });
    let mut str_length = 0;
    let printed_items: Vec<String> = formatted_items
        .iter()
        .take_while(|&item| {
            str_length += item.len();
            str_length < 1024
        })
        .map(|item| item.into())
        .collect();

    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if printed_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"No items found in the map\n\"".to_string(),
            );
            return Ok(());
        }
        main_engine.send_server_command(
            None::<Client>,
            format!("print \"{}\n\"", printed_items.join("\n")),
        );

        let remaining_items: Vec<String> = formatted_items
            .iter()
            .skip(printed_items.len())
            .map(|item| item.into())
            .collect();

        if !remaining_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"Check server console for other items\n\"\n".to_string(),
            );
            remaining_items
                .into_iter()
                .for_each(|item| main_engine.com_printf(item));
        }

        Ok(())
    })
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "force_weapon_respawn_time")]
pub(crate) fn force_weapon_respawn_time(py: Python<'_>, respawn_time: i32) -> PyResult<bool> {
    if respawn_time < 0 {
        return Err(PyValueError::new_err(
            "respawn time needs to be an integer 0 or greater",
        ));
    }

    py.allow_threads(move || {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_respawning_weapon())
            .for_each(|mut game_entity| game_entity.set_respawn_time(respawn_time))
    });

    Ok(true)
}

/// get a list of entities that target a given entity
#[pyfunction]
#[pyo3(name = "get_targetting_entities")]
pub(crate) fn get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
    if entity_id < 0 || entity_id >= MAX_GENTITIES as i32 {
        return Err(PyValueError::new_err(format!(
            "entity_id need to be between 0 and {}.",
            MAX_GENTITIES - 1
        )));
    }

    py.allow_threads(move || {
        GameEntity::try_from(entity_id).map_or_else(
            |_| Ok(vec![]),
            |entity| Ok(entity.get_targetting_entity_ids()),
        )
    })
}
