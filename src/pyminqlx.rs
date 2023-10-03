use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use crate::commands::cmd_py_command;
#[cfg(test)]
use crate::hooks::mock_hooks::{
    shinqlx_client_spawn, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command,
};
#[cfg(not(test))]
use crate::hooks::{
    shinqlx_client_spawn, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command,
};
use crate::hooks::{shinqlx_com_printf, shinqlx_set_configstring};
use crate::MAIN_ENGINE;
use core::sync::atomic::AtomicI32;
use core::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::Lazy;
use swap_arc::SwapArcOption;

use crate::current_level::CurrentLevel;
use crate::game_item::GameItem;
use crate::quake_live_engine::{
    AddCommand, ComPrintf, ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SendServerCommand,
    SetCVarForced, SetCVarLimit,
};
use pyo3::basic::CompareOp;
use pyo3::exceptions::{PyEnvironmentError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::{append_to_inittab, prepare_freethreaded_python};

static ALLOW_FREE_CLIENT: AtomicI32 = AtomicI32::new(-1);

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    let Some(ref client_command_handler) = *CLIENT_COMMAND_HANDLER.load() else {
        return Some(cmd.as_ref().into());
    };

    Python::with_gil(
        |py| match client_command_handler.call1(py, (client_id, cmd.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "client_command_handler returned an error.");
                Some(cmd.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn server_command_dispatcher<T>(client_id: Option<i32>, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    let Some(ref server_command_handler) = *SERVER_COMMAND_HANDLER.load() else {
        return Some(cmd.as_ref().into());
    };

    Python::with_gil(|py| {
        match server_command_handler.call1(py, (client_id.unwrap_or(-1), cmd.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "server_command_handler returned an error.");
                Some(cmd.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        }
    })
}

pub(crate) fn frame_dispatcher() {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref frame_handler) = *FRAME_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = frame_handler.call0(py);
        if result.is_err() {
            error!(target: "shinqlx", "frame_handler returned an error.");
        }
    });
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return None;
    }

    let Some(ref client_connect_handler) = *PLAYER_CONNECT_HANDLER.load() else {
        return None;
    };

    ALLOW_FREE_CLIENT.store(client_id, Ordering::Relaxed);

    let result: Option<String> =
        Python::with_gil(
            |py| match client_connect_handler.call1(py, (client_id, is_bot)) {
                Err(_) => None,
                Ok(returned) => match returned.extract::<String>(py) {
                    Err(_) => match returned.extract::<bool>(py) {
                        Err(_) => None,
                        Ok(result_bool) => {
                            if !result_bool {
                                Some("You are banned from this server.".into())
                            } else {
                                None
                            }
                        }
                    },
                    Ok(result_string) => Some(result_string),
                },
            },
        );

    ALLOW_FREE_CLIENT.store(-1, Ordering::Relaxed);

    result
}

pub(crate) fn client_disconnect_dispatcher<T>(client_id: i32, reason: T)
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_disconnect_handler) = *PLAYER_DISCONNECT_HANDLER.load() else {
        return;
    };

    ALLOW_FREE_CLIENT.store(client_id, Ordering::Relaxed);
    Python::with_gil(|py| {
        let result = client_disconnect_handler.call1(py, (client_id, reason.as_ref()));
        if result.is_err() {
            error!(target: "shinqlx", "client_disconnect_handler returned an error.");
        }
    });
    ALLOW_FREE_CLIENT.store(-1, Ordering::Relaxed);
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_loaded_handler) = *PLAYER_LOADED_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let returned_value = client_loaded_handler.call1(py, (client_id,));
        if returned_value.is_err() {
            error!(target: "shinqlx", "client_loaded_handler returned an error.");
        }
    });
}

pub(crate) fn new_game_dispatcher(restart: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref new_game_handler) = *NEW_GAME_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = new_game_handler.call1(py, (restart,));
        if result.is_err() {
            error!(target: "shinqlx", "new_game_handler returned an error.");
        }
    });
}

pub(crate) fn set_configstring_dispatcher<T, U>(index: T, value: U) -> Option<String>
where
    T: Into<u32>,
    U: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(value.as_ref().into());
    }

    let Some(ref set_configstring_handler) = *SET_CONFIGSTRING_HANDLER.load() else {
        return Some(value.as_ref().into());
    };

    Python::with_gil(|py| {
        match set_configstring_handler.call1(py, (index.into(), value.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "set_configstring_handler returned an error.");
                Some(value.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(value.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(value.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        }
    })
}

pub(crate) fn rcon_dispatcher<T>(cmd: T)
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref rcon_handler) = *RCON_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = rcon_handler.call1(py, (cmd.as_ref(),));
        if result.is_err() {
            error!(target: "shinqlx", "rcon_handler returned an error.");
        }
    });
}

pub(crate) fn console_print_dispatcher<T>(text: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(text.as_ref().into());
    }

    let Some(ref console_print_handler) = *CONSOLE_PRINT_HANDLER.load() else {
        return Some(text.as_ref().into());
    };

    Python::with_gil(
        |py| match console_print_handler.call1(py, (text.as_ref(),)) {
            Err(_) => {
                error!(target: "shinqlx", "console_print_handler returned an error.");
                Some(text.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(text.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(text.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_spawn_handler) = *PLAYER_SPAWN_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = client_spawn_handler.call1(py, (client_id,));
        if result.is_err() {
            error!(target: "shinqlx", "client_spawn_handler returned an error.");
        }
    });
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref kamikaze_use_handler) = *KAMIKAZE_USE_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = kamikaze_use_handler.call1(py, (client_id,));
        if result.is_err() {
            error!(target: "shinqlx", "kamikaze_use_handler returned an error.");
        }
    });
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref kamikaze_explode_handler) = *KAMIKAZE_EXPLODE_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let result = kamikaze_explode_handler.call1(py, (client_id, is_used_on_demand));
        if result.is_err() {
            error!(target: "shinqlx", "kamikaze_explode_handler returned an error.");
        }
    });
}

pub(crate) fn damage_dispatcher(
    target_client_id: i32,
    attacker_client_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref damage_handler) = *DAMAGE_HANDLER.load() else {
        return;
    };

    Python::with_gil(|py| {
        let returned_value = damage_handler.call1(
            py,
            (
                target_client_id,
                attacker_client_id,
                damage,
                dflags,
                means_of_death,
            ),
        );
        if returned_value.is_err() {
            error!(target: "shinqlx", "damage_handler returned an error.");
        }
    });
}

#[cfg(test)]
mod pyminqlx_dispatcher_tests {
    use super::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher, PYMINQLX_INITIALIZED,
    };
    use crate::prelude::*;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;

    #[test]
    fn client_command_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn client_command_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn server_command_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn server_command_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn frame_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        frame_dispatcher();
    }

    #[test]
    fn frame_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        frame_dispatcher();
    }

    #[test]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_connect_dispatcher(123, false);
        assert_eq!(result, None);
    }

    #[test]
    fn client_connect_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let result = client_connect_dispatcher(123, false);
        assert_eq!(result, None);
    }

    #[test]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_disconnect_dispatcher(123, "asdf");
    }

    #[test]
    fn client_disconnect_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        client_disconnect_dispatcher(123, "ragequit");
    }

    #[test]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_loaded_dispatcher(123);
    }

    #[test]
    fn client_loaded_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        client_loaded_dispatcher(123);
    }

    #[test]
    fn new_game_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        new_game_dispatcher(false);
    }

    #[test]
    fn new_game_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        new_game_dispatcher(true);
    }

    #[test]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn set_configstring_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn rcon_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        rcon_dispatcher("asdf");
    }

    #[test]
    fn rcon_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        rcon_dispatcher("asdf");
    }

    #[test]
    fn console_print_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn console_print_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    fn client_spawn_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_spawn_dispatcher(123);
    }

    #[test]
    fn client_spawn_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        client_spawn_dispatcher(123);
    }

    #[test]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    fn kamikaze_use_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    fn kamikaze_explode_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        kamikaze_explode_dispatcher(123, true);
    }

    #[test]
    fn damage_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        damage_dispatcher(
            123,
            None,
            666,
            DAMAGE_NO_PROTECTION as i32,
            meansOfDeath_t::MOD_TRIGGER_HURT as i32,
        );
    }

    #[test]
    fn damage_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }
}

/// Information about a player, such as Steam ID, name, client ID, and whatnot.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerInfo", get_all)]
#[derive(Debug)]
#[allow(unused)]
struct PlayerInfo {
    /// The player's client ID.
    client_id: i32,
    /// The player's name.
    name: String,
    /// The player's connection state.
    connection_state: i32,
    /// The player's userinfo.
    userinfo: String,
    /// The player's 64-bit representation of the Steam ID.
    steam_id: u64,
    /// The player's team.
    team: i32,
    /// The player's privileges.
    privileges: i32,
}

#[pymethods]
impl PlayerInfo {
    fn __str__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
    }

    fn __repr__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
    }
}

impl TryFrom<i32> for PlayerInfo {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        let game_entity_result = GameEntity::try_from(client_id);
        match game_entity_result {
            Err(_) => Ok(PlayerInfo {
                client_id,
                name: Default::default(),
                connection_state: clientState_t::CS_FREE as i32,
                userinfo: Default::default(),
                steam_id: 0,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: -1,
            }),
            Ok(game_entity) => {
                let Ok(client) = Client::try_from(client_id) else {
                    return Ok(PlayerInfo {
                        client_id,
                        name: game_entity.get_player_name(),
                        connection_state: clientState_t::CS_FREE as i32,
                        userinfo: Default::default(),
                        steam_id: 0,
                        team: game_entity.get_team() as i32,
                        privileges: game_entity.get_privileges() as i32,
                    });
                };
                Ok(PlayerInfo {
                    client_id,
                    name: game_entity.get_player_name(),
                    connection_state: client.get_state() as i32,
                    userinfo: client.get_user_info(),
                    steam_id: client.get_steam_id(),
                    team: game_entity.get_team() as i32,
                    privileges: game_entity.get_privileges() as i32,
                })
            }
        }
    }
}

/// Returns a dictionary with information about a player by ID.
#[pyfunction(name = "player_info")]
fn get_player_info(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerInfo>> {
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
            return Ok(PlayerInfo::try_from(client_id).ok());
        };

        let allowed_free_client_id = ALLOW_FREE_CLIENT.load(Ordering::Relaxed);
        if allowed_free_client_id != client_id && client.get_state() == clientState_t::CS_FREE {
            warn!(
                target: "shinqlx",
                "WARNING: get_player_info called for CS_FREE client {}.",
                client_id
            );
            return Ok(None);
        }

        Ok(PlayerInfo::try_from(client_id).ok())
    })
}

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
fn get_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
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
                        _ => Some(PlayerInfo::try_from(client_id).ok()),
                    },
                )
            })
            .collect();

        Ok(result)
    })
}

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
fn get_userinfo(py: Python<'_>, client_id: i32) -> PyResult<Option<String>> {
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

    py.allow_threads(move || match Client::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(client) => {
            let allowed_free_client_id = ALLOW_FREE_CLIENT.load(Ordering::Relaxed);
            if allowed_free_client_id != client_id && client.get_state() == clientState_t::CS_FREE {
                Ok(None)
            } else {
                Ok(Some(client.get_user_info()))
            }
        }
    })
}

/// Sends a server command to either one specific client or all the clients.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
fn send_server_command(py: Python<'_>, client_id: Option<i32>, cmd: &str) -> PyResult<bool> {
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

            match Client::try_from(actual_client_id) {
                Err(_) => Ok(false),
                Ok(client) => {
                    if client.get_state() != clientState_t::CS_ACTIVE {
                        Ok(false)
                    } else {
                        #[allow(clippy::unnecessary_to_owned)]
                        shinqlx_send_server_command(Some(client), cmd.to_string());
                        Ok(true)
                    }
                }
            }
        }
    }
}

/// Tells the server to process a command from a specific client.
#[pyfunction]
#[pyo3(name = "client_command")]
fn client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<bool> {
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

    match Client::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(client) => {
            if [clientState_t::CS_FREE, clientState_t::CS_ZOMBIE].contains(&client.get_state()) {
                Ok(false)
            } else {
                shinqlx_execute_client_command(Some(client), cmd.to_string(), true);
                Ok(true)
            }
        }
    }
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
fn console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
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

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
fn get_cvar(py: Python<'_>, cvar: &str) -> PyResult<Option<String>> {
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

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
fn set_cvar(py: Python<'_>, cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
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

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
fn set_cvar_limit(
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

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
fn kick(py: Python<'_>, client_id: i32, reason: Option<&str>) -> PyResult<()> {
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

    py.allow_threads(move || match Client::try_from(client_id) {
        Err(_) => Err(PyValueError::new_err(
            "client_id must be None or the ID of an active player.",
        )),
        Ok(mut client) => {
            if client.get_state() != clientState_t::CS_ACTIVE {
                return Err(PyValueError::new_err(
                    "client_id must be None or the ID of an active player.",
                ));
            }
            let reason_str = if reason.unwrap_or("was kicked.").is_empty() {
                "was kicked."
            } else {
                reason.unwrap_or("was kicked.")
            };
            #[allow(clippy::unnecessary_to_owned)]
            shinqlx_drop_client(&mut client, reason_str.to_string());
            Ok(())
        }
    })
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
fn console_print(py: Python<'_>, text: &str) {
    py.allow_threads(move || {
        let formatted_string = format!("{}\n", text);
        shinqlx_com_printf(formatted_string.as_str());
    })
}

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
fn get_configstring(py: Python<'_>, config_id: u32) -> PyResult<String> {
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

        Ok(main_engine.get_configstring(config_id as i16))
    })
}

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
fn set_configstring(py: Python<'_>, config_id: u32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        shinqlx_set_configstring(config_id, value);

        Ok(())
    })
}

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
fn force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
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
                    .is_ok_and(|client| client.get_state() == clientState_t::CS_ACTIVE)
            })
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_state(pass));
    });

    Ok(true)
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
fn add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.add_command(command, cmd_py_command);

        Ok(())
    })
}

static CLIENT_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static SERVER_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static FRAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static PLAYER_CONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_LOADED_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_DISCONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static CUSTOM_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static NEW_GAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static SET_CONFIGSTRING_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static RCON_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static CONSOLE_PRINT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_SPAWN_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static KAMIKAZE_USE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static KAMIKAZE_EXPLODE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static DAMAGE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
fn register_handler(py: Python<'_>, event: &str, handler: Option<Py<PyAny>>) -> PyResult<()> {
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

#[pyclass]
struct Vector3Iter {
    iter: vec::IntoIter<i32>,
}

#[pymethods]
impl Vector3Iter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<i32> {
        slf.iter.next()
    }
}

/// A three-dimensional vector.
#[pyclass(name = "Vector3", module = "minqlx", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, Default)]
struct Vector3(
    #[pyo3(name = "x")] i32,
    #[pyo3(name = "y")] i32,
    #[pyo3(name = "z")] i32,
);

#[pymethods]
impl Vector3 {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 3 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all three dimensions",
            ));
        }

        if values.len() > 3 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than three dimensions",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Vector3 values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __repr__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<Vector3Iter>> {
        let iter_vec = vec![slf.0, slf.1, slf.2];
        let iter = Vector3Iter {
            iter: iter_vec.into_iter(),
        };
        Py::new(slf.py(), iter)
    }
}

impl From<(f32, f32, f32)> for Vector3 {
    fn from(value: (f32, f32, f32)) -> Self {
        Self(value.0 as i32, value.1 as i32, value.2 as i32)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
pub(crate) mod pyminqlx_setup_fixture {
    use crate::pyminqlx::pyminqlx_module;
    use pyo3::ffi::Py_IsInitialized;
    use pyo3::{append_to_inittab, prepare_freethreaded_python};
    use rstest::fixture;

    #[fixture]
    #[once]
    pub(crate) fn pyminqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyminqlx_module);
            prepare_freethreaded_python();
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod vector3_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn vector3_tuple_test(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let minqlx_module = py.import("_minqlx").unwrap();
            let vector3 = minqlx_module.getattr("Vector3").unwrap();
            let tuple = py.import("builtins").unwrap().getattr("tuple").unwrap();
            assert!(vector3.is_instance(tuple.get_type()).unwrap());
        });
    }

    #[rstest]
    fn vector3_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Vector3((0, 42, 666))
            "#,
                None,
                None,
            );
            assert!(
                vector3_constructor.is_ok(),
                "{}",
                vector3_constructor.err().unwrap()
            );
        });
    }
}

/// A struct sequence containing all the weapons in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "Weapons", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Weapons(
    #[pyo3(name = "g")] i32,
    #[pyo3(name = "mg")] i32,
    #[pyo3(name = "sg")] i32,
    #[pyo3(name = "gl")] i32,
    #[pyo3(name = "rl")] i32,
    #[pyo3(name = "lg")] i32,
    #[pyo3(name = "rg")] i32,
    #[pyo3(name = "pg")] i32,
    #[pyo3(name = "bfg")] i32,
    #[pyo3(name = "gh")] i32,
    #[pyo3(name = "ng")] i32,
    #[pyo3(name = "pl")] i32,
    #[pyo3(name = "cg")] i32,
    #[pyo3(name = "hmg")] i32,
    #[pyo3(name = "hands")] i32,
);

impl From<[i32; 15]> for Weapons {
    fn from(value: [i32; 15]) -> Self {
        Self(
            value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
            value[8], value[9], value[10], value[11], value[12], value[13], value[14],
        )
    }
}

impl From<Weapons> for [i32; 15] {
    fn from(value: Weapons) -> Self {
        [
            value.0, value.1, value.2, value.3, value.4, value.5, value.6, value.7, value.8,
            value.9, value.10, value.11, value.12, value.13, value.14,
        ]
    }
}

#[pymethods]
impl Weapons {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 15 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 15 weapons",
            ));
        }

        if values.len() > 15 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 15 weapons",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Weapons values need to be boolean"));
        }

        Ok(Self::from(
            <Vec<i32> as TryInto<[i32; 15]>>::try_into(
                results
                    .into_iter()
                    .map(|value| value.unwrap_or(0))
                    .collect::<Vec<i32>>(),
            )
            .unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Weapons(g={}, mg={}, sg={}, gl={}, rl={}, lg={}, rg={}, pg={}, bfg={}, gh={}, ng={}, pl={}, cg={}, hmg={}, hands={})",
        self.0, self.1, self.2, self.3, self.4, self.5, self.5, self.7, self.8, self.9, self.10, self.11, self.12, self.13, self.14)
    }

    fn __repr__(&self) -> String {
        format!("Weapons(g={}, mg={}, sg={}, gl={}, rl={}, lg={}, rg={}, pg={}, bfg={}, gh={}, ng={}, pl={}, cg={}, hmg={}, hands={})",
        self.0, self.1, self.2, self.3, self.4, self.5, self.5, self.7, self.8, self.9, self.10, self.11, self.12, self.13, self.14)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod weapons_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn weapons_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor =py.run(r#"
import _minqlx
weapons = _minqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False))
            "#, None, None);
            assert!(
                weapons_constructor.is_ok(),
                "{}",
                weapons_constructor.err().unwrap()
            );
        });
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod ammo_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn ammo_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14))
            "#,
                None,
                None,
            );
            assert!(
                ammo_constructor.is_ok(),
                "{}",
                ammo_constructor.err().unwrap()
            );
        });
    }
}

/// A struct sequence containing all the powerups in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "Powerups", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Powerups(
    #[pyo3(name = "quad")] i32,
    #[pyo3(name = "battlesuit")] i32,
    #[pyo3(name = "haste")] i32,
    #[pyo3(name = "invisibility")] i32,
    #[pyo3(name = "regeneration")] i32,
    #[pyo3(name = "invulnerability")] i32,
);

impl From<[i32; 6]> for Powerups {
    fn from(value: [i32; 6]) -> Self {
        Self(value[0], value[1], value[2], value[3], value[4], value[5])
    }
}

impl From<Powerups> for [i32; 6] {
    fn from(value: Powerups) -> Self {
        [value.0, value.1, value.2, value.3, value.4, value.5]
    }
}

#[pymethods]
impl Powerups {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 6 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 6 powerups",
            ));
        }

        if values.len() > 6 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 6 powerups",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Powerups values need to be integer"));
        }

        Ok(Self::from(
            <Vec<i32> as TryInto<[i32; 6]>>::try_into(
                results
                    .into_iter()
                    .map(|value| value.unwrap_or(0))
                    .collect::<Vec<i32>>(),
            )
            .unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5)
    }

    fn __repr__(&self) -> String {
        format!("Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod powerups_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn powerups_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Powerups((0, 1, 2, 3, 4, 5))
            "#,
                None,
                None,
            );
            assert!(
                powerups_constructor.is_ok(),
                "{}",
                powerups_constructor.err().unwrap(),
            );
        });
    }
}

#[pyclass]
#[pyo3(module = "minqlx", name = "Holdable")]
#[derive(PartialEq, Debug, Clone, Copy)]
enum Holdable {
    None = 0,
    Teleporter = 27,
    MedKit = 28,
    Kamikaze = 37,
    Portal = 38,
    Invulnerability = 39,
    Flight = 34,
    Unknown = 666,
}

impl From<i32> for Holdable {
    fn from(value: i32) -> Self {
        match value {
            0 => Holdable::None,
            27 => Holdable::Teleporter,
            28 => Holdable::MedKit,
            34 => Holdable::Flight,
            37 => Holdable::Kamikaze,
            38 => Holdable::Portal,
            39 => Holdable::Invulnerability,
            _ => Holdable::Unknown,
        }
    }
}

impl From<Holdable> for i32 {
    fn from(value: Holdable) -> Self {
        match value {
            Holdable::None => 0,
            Holdable::Teleporter => 27,
            Holdable::MedKit => 28,
            Holdable::Flight => 24,
            Holdable::Kamikaze => 37,
            Holdable::Portal => 38,
            Holdable::Invulnerability => 39,
            Holdable::Unknown => 0,
        }
    }
}

/// A struct sequence containing parameters for the flight holdable item.
#[pyclass]
#[pyo3(module = "minqlx", name = "Flight", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Flight(
    #[pyo3(name = "fuel")] i32,
    #[pyo3(name = "max_fuel")] i32,
    #[pyo3(name = "thrust")] i32,
    #[pyo3(name = "refuel")] i32,
);

impl From<Flight> for [i32; 4] {
    fn from(flight: Flight) -> Self {
        [flight.0, flight.1, flight.2, flight.3]
    }
}

#[pymethods]
impl Flight {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 4 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 4 flight parameters",
            ));
        }

        if values.len() > 4 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 4 flight parameters",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Flight values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
            results[3].unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod flight_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn flight_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Flight((0, 1, 2, 3))
            "#,
                None,
                None,
            );
            assert!(
                flight_constructor.is_ok(),
                "{}",
                flight_constructor.err().unwrap()
            );
        });
    }
}

/// Information about a player's state in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerState", get_all)]
#[allow(unused)]
struct PlayerState {
    /// Whether the player's alive or not.
    is_alive: bool,
    /// The player's position.
    position: Vector3,
    /// The player's velocity.
    velocity: Vector3,
    /// The player's health.
    health: i32,
    /// The player's armor.
    armor: i32,
    /// Whether the player has noclip or not.
    noclip: bool,
    /// The weapon the player is currently using.
    weapon: i32,
    /// The player's weapons.
    weapons: Weapons,
    /// The player's weapon ammo.
    ammo: Weapons,
    ///The player's powerups.
    powerups: Powerups,
    /// The player's holdable item.
    holdable: Option<String>,
    /// A struct sequence with flight parameters.
    flight: Flight,
    /// Whether the player is currently chatting.
    is_chatting: bool,
    /// Whether the player is frozen(freezetag).
    is_frozen: bool,
}

#[pymethods]
impl PlayerState {
    fn __str__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
            self.is_alive,
            self.position.__str__(),
            self.velocity.__str__(),
            self.health,
            self.armor,
            self.noclip,
            self.weapon,
            self.weapons.__str__(),
            self.ammo.__str__(),
            self.powerups.__str__(),
            match self.holdable.as_ref() {
                Some(value) => value,
                None => "None",
            },
            self.flight.__str__(),
            self.is_chatting,
            self.is_frozen)
    }

    fn __repr__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
            self.is_alive,
            self.position.__str__(),
            self.velocity.__str__(),
            self.health,
            self.armor,
            self.noclip,
            self.weapon,
            self.weapons.__str__(),
            self.ammo.__str__(),
            self.powerups.__str__(),
            match self.holdable.as_ref() {
                Some(value) => value,
                None => "None",
            },
            self.flight.__str__(),
            self.is_chatting,
            self.is_frozen)
    }
}

impl From<GameEntity> for PlayerState {
    fn from(game_entity: GameEntity) -> Self {
        let game_client = game_entity.get_game_client().unwrap();
        let position = game_client.get_position();
        let velocity = game_client.get_velocity();
        Self {
            is_alive: game_client.is_alive(),
            position: Vector3::from(position),
            velocity: Vector3::from(velocity),
            health: game_entity.get_health(),
            armor: game_client.get_armor(),
            noclip: game_client.get_noclip(),
            weapon: game_client.get_weapon().into(),
            weapons: Weapons::from(game_client.get_weapons()),
            ammo: Weapons::from(game_client.get_ammos()),
            powerups: Powerups::from(game_client.get_powerups()),
            holdable: holdable_from(game_client.get_holdable().into()),
            flight: Flight(
                game_client.get_current_flight_fuel(),
                game_client.get_max_flight_fuel(),
                game_client.get_flight_thrust(),
                game_client.get_flight_refuel(),
            ),
            is_chatting: game_client.is_chatting(),
            is_frozen: game_client.is_frozen(),
        }
    }
}

fn holdable_from(holdable: Holdable) -> Option<String> {
    match holdable {
        Holdable::None => None,
        Holdable::Teleporter => Some("teleporter".into()),
        Holdable::MedKit => Some("medkit".into()),
        Holdable::Kamikaze => Some("kamikaze".into()),
        Holdable::Portal => Some("portal".into()),
        Holdable::Invulnerability => Some("invulnerability".into()),
        Holdable::Flight => Some("flight".into()),
        Holdable::Unknown => Some("unknown".into()),
    }
}

/// Get information about the player's state in the game.
#[pyfunction]
#[pyo3(name = "player_state")]
fn player_state(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerState>> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(game_entity) => {
            if game_entity.get_game_client().is_err() {
                return Ok(None);
            }
            Ok(Some(PlayerState::from(game_entity)))
        }
    })
}

/// A player's score and some basic stats.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerStats", get_all)]
#[allow(unused)]
struct PlayerStats {
    /// The player's primary score.
    score: i32,
    /// The player's number of kills.
    kills: i32,
    /// The player's number of deaths.
    deaths: i32,
    /// The player's total damage dealt.
    damage_dealt: i32,
    /// The player's total damage taken.
    damage_taken: i32,
    /// The time in milliseconds the player has on a team since the game started.
    time: i32,
    /// The player's ping.
    ping: i32,
}

#[pymethods]
impl PlayerStats {
    fn __str__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
            self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }

    fn __repr__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
            self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }
}

impl From<GameClient> for PlayerStats {
    fn from(game_client: GameClient) -> Self {
        Self {
            score: game_client.get_score(),
            kills: game_client.get_kills(),
            deaths: game_client.get_deaths(),
            damage_dealt: game_client.get_damage_dealt(),
            damage_taken: game_client.get_damage_taken(),
            time: game_client.get_time_on_team(),
            ping: game_client.get_ping(),
        }
    }
}

/// Get some player stats.
#[pyfunction]
#[pyo3(name = "player_stats")]
fn player_stats(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerStats>> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(game_entity) => Ok(Some(PlayerStats::from(
            game_entity.get_game_client().unwrap(),
        ))),
    })
}

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
fn set_position(py: Python<'_>, client_id: i32, position: Vector3) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut mutable_client = game_entity.get_game_client().unwrap();
            mutable_client.set_position((position.0 as f32, position.1 as f32, position.2 as f32));
            Ok(true)
        }
    })
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
fn set_velocity(py: Python<'_>, client_id: i32, velocity: Vector3) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut mutable_client = game_entity.get_game_client().unwrap();
            mutable_client.set_velocity((velocity.0 as f32, velocity.1 as f32, velocity.2 as f32));
            Ok(true)
        }
    })
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
fn noclip(py: Python<'_>, client_id: i32, activate: bool) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            if game_client.get_noclip() == activate {
                Ok(false)
            } else {
                game_client.set_noclip(activate);
                Ok(true)
            }
        }
    })
}

/// Sets a player's health.
#[pyfunction]
#[pyo3(name = "set_health")]
fn set_health(py: Python<'_>, client_id: i32, health: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_entity = game_entity;
            game_entity.set_health(health);
            Ok(true)
        }
    })
}

/// Sets a player's armor.
#[pyfunction]
#[pyo3(name = "set_armor")]
fn set_armor(py: Python<'_>, client_id: i32, armor: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_armor(armor);
            Ok(true)
        }
    })
}

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
fn set_weapons(py: Python<'_>, client_id: i32, weapons: Weapons) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_weapons(weapons.into());
            Ok(true)
        }
    })
}

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
fn set_weapon(py: Python<'_>, client_id: i32, weapon: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_weapon(weapon);
            Ok(true)
        }
    })
}

/// Sets a player's ammo.
#[pyfunction]
#[pyo3(name = "set_ammo")]
fn set_ammo(py: Python<'_>, client_id: i32, ammos: Weapons) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_ammos(ammos.into());
            Ok(true)
        }
    })
}

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
fn set_powerups(py: Python<'_>, client_id: i32, powerups: Powerups) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_powerups(powerups.into());
            Ok(true)
        }
    })
}

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
fn set_holdable(py: Python<'_>, client_id: i32, holdable: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            let ql_holdable = Holdable::from(holdable);
            game_client.set_holdable(ql_holdable);
            Ok(true)
        }
    })
}

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
fn drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(mut game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.remove_kamikaze_flag();
            if Holdable::from(game_client.get_holdable()) == Holdable::None {
                return Ok(false);
            }
            game_entity.drop_holdable();
            Ok(true)
        }
    })
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
fn set_flight(py: Python<'_>, client_id: i32, flight: Flight) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_flight::<[i32; 4]>(flight.into());
            Ok(true)
        }
    })
}

/// Makes player invulnerable for limited time.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
fn set_invulnerability(py: Python<'_>, client_id: i32, time: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_invulnerability(time);
            Ok(true)
        }
    })
}

/// Sets a player's score.
#[pyfunction]
#[pyo3(name = "set_score")]
fn set_score(py: Python<'_>, client_id: i32, score: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => {
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.set_score(score);
            Ok(true)
        }
    })
}

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
fn callvote(py: Python<'_>, vote: &str, vote_disp: &str, vote_time: Option<i32>) {
    py.allow_threads(move || {
        let Ok(mut current_level) = CurrentLevel::try_get() else {
            return;
        };
        current_level.callvote(vote, vote_disp, vote_time);
    })
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
fn allow_single_player(py: Python<'_>, allow: bool) {
    py.allow_threads(move || {
        let Ok(mut current_level) = CurrentLevel::try_get() else {
            return;
        };
        current_level.set_training_map(allow);
    })
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
fn player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => match game_entity.get_game_client() {
            Err(_) => Ok(false),
            Ok(game_client) => {
                let mut game_client = game_client;
                game_client.spawn();
                shinqlx_client_spawn(game_entity);
                Ok(true)
            }
        },
    })
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
fn set_privileges(py: Python<'_>, client_id: i32, privileges: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => match game_entity.get_game_client() {
            Err(_) => Ok(false),
            Ok(game_client) => {
                let mut game_client = game_client;
                game_client.set_privileges(privileges);
                Ok(true)
            }
        },
    })
}

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
fn destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
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
fn spawn_item(py: Python<'_>, item_id: i32, x: i32, y: i32, z: i32) -> PyResult<bool> {
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
fn remove_dropped_items(py: Python<'_>) -> PyResult<bool> {
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
fn slay_with_mod(py: Python<'_>, client_id: i32, mean_of_death: i32) -> PyResult<bool> {
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

    py.allow_threads(move || match GameEntity::try_from(client_id) {
        Err(_) => Ok(false),
        Ok(game_entity) => match game_entity.get_game_client() {
            Err(_) => Ok(false),
            Ok(_) => {
                if game_entity.get_health() > 0 {
                    let mut mut_entity = game_entity;
                    mut_entity.slay_with_mod(mean_of_death.try_into().unwrap());
                }
                Ok(true)
            }
        },
    })
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
#[allow(unused_variables)]
#[pyfunction]
#[pyo3(name = "replace_items")]
#[pyo3(signature = (item1, item2))]
fn replace_items(py: Python<'_>, item1: Py<PyAny>, item2: Py<PyAny>) -> PyResult<bool> {
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
fn dev_print_items(py: Python<'_>) -> PyResult<()> {
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
            main_engine
                .send_server_command(None::<Client>, "print \"No items found in the map\n\"");
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
            main_engine.send_server_command(
                None::<Client>,
                "print \"Check server console for other items\n\"\n",
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
fn force_weapon_respawn_time(py: Python<'_>, respawn_time: i32) -> PyResult<bool> {
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
fn get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
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

// Used primarily in Python, but defined here and added using PyModule_AddIntMacro().
#[allow(non_camel_case_types)]
enum PythonReturnCodes {
    RET_NONE,
    RET_STOP,       // Stop execution of event handlers within Python.
    RET_STOP_EVENT, // Only stop the event, but let other handlers process it.
    RET_STOP_ALL,   // Stop execution at an engine level. SCARY STUFF!
    RET_USAGE,      // Used for commands. Replies to the channel with a command's usage.
}

#[allow(non_camel_case_types)]
enum PythonPriorities {
    PRI_HIGHEST,
    PRI_HIGH,
    PRI_NORMAL,
    PRI_LOW,
    PRI_LOWEST,
}

#[pymodule]
#[pyo3(name = "shinqlx")]
fn pyshinqlx_module(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    Ok(())
}

#[pymodule]
#[pyo3(name = "_minqlx")]
fn pyminqlx_module(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_player_info, m)?)?;
    m.add_function(wrap_pyfunction!(get_players_info, m)?)?;
    m.add_function(wrap_pyfunction!(get_userinfo, m)?)?;
    m.add_function(wrap_pyfunction!(send_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(client_command, m)?)?;
    m.add_function(wrap_pyfunction!(console_command, m)?)?;
    m.add_function(wrap_pyfunction!(get_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(set_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(set_cvar_limit, m)?)?;
    m.add_function(wrap_pyfunction!(kick, m)?)?;
    m.add_function(wrap_pyfunction!(console_print, m)?)?;
    m.add_function(wrap_pyfunction!(get_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(force_vote, m)?)?;
    m.add_function(wrap_pyfunction!(add_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(register_handler, m)?)?;
    m.add_function(wrap_pyfunction!(player_state, m)?)?;
    m.add_function(wrap_pyfunction!(player_stats, m)?)?;
    m.add_function(wrap_pyfunction!(set_position, m)?)?;
    m.add_function(wrap_pyfunction!(set_velocity, m)?)?;
    m.add_function(wrap_pyfunction!(noclip, m)?)?;
    m.add_function(wrap_pyfunction!(set_health, m)?)?;
    m.add_function(wrap_pyfunction!(set_armor, m)?)?;
    m.add_function(wrap_pyfunction!(set_weapons, m)?)?;
    m.add_function(wrap_pyfunction!(set_weapon, m)?)?;
    m.add_function(wrap_pyfunction!(set_ammo, m)?)?;
    m.add_function(wrap_pyfunction!(set_powerups, m)?)?;
    m.add_function(wrap_pyfunction!(set_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(drop_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(set_flight, m)?)?;
    m.add_function(wrap_pyfunction!(set_invulnerability, m)?)?;
    m.add_function(wrap_pyfunction!(set_score, m)?)?;
    m.add_function(wrap_pyfunction!(callvote, m)?)?;
    m.add_function(wrap_pyfunction!(allow_single_player, m)?)?;
    m.add_function(wrap_pyfunction!(player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(set_privileges, m)?)?;
    m.add_function(wrap_pyfunction!(destroy_kamikaze_timers, m)?)?;
    m.add_function(wrap_pyfunction!(spawn_item, m)?)?;
    m.add_function(wrap_pyfunction!(remove_dropped_items, m)?)?;
    m.add_function(wrap_pyfunction!(slay_with_mod, m)?)?;
    m.add_function(wrap_pyfunction!(replace_items, m)?)?;
    m.add_function(wrap_pyfunction!(dev_print_items, m)?)?;
    m.add_function(wrap_pyfunction!(force_weapon_respawn_time, m)?)?;
    m.add_function(wrap_pyfunction!(get_entity_targets, m)?)?;

    m.add("__version__", env!("SHINQLX_VERSION"))?;
    m.add("DEBUG", cfg!(debug_assertions))?;

    // Set a bunch of constants. We set them here because if you define functions in Python that use module
    // constants as keyword defaults, we have to always make sure they're exported first, and fuck that.
    m.add("RET_NONE", PythonReturnCodes::RET_NONE as i32)?;
    m.add("RET_STOP", PythonReturnCodes::RET_STOP as i32)?;
    m.add("RET_STOP_EVENT", PythonReturnCodes::RET_STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", PythonReturnCodes::RET_STOP_ALL as i32)?;
    m.add("RET_USAGE", PythonReturnCodes::RET_USAGE as i32)?;
    m.add("PRI_HIGHEST", PythonPriorities::PRI_HIGHEST as i32)?;
    m.add("PRI_HIGH", PythonPriorities::PRI_HIGH as i32)?;
    m.add("PRI_NORMAL", PythonPriorities::PRI_NORMAL as i32)?;
    m.add("PRI_LOW", PythonPriorities::PRI_LOW as i32)?;
    m.add("PRI_LOWEST", PythonPriorities::PRI_LOWEST as i32)?;

    // Cvar flags.
    m.add("CVAR_ARCHIVE", cvar_flags::CVAR_ARCHIVE as i32)?;
    m.add("CVAR_USERINFO", cvar_flags::CVAR_USERINFO as i32)?;
    m.add("CVAR_SERVERINFO", cvar_flags::CVAR_SERVERINFO as i32)?;
    m.add("CVAR_SYSTEMINFO", cvar_flags::CVAR_SYSTEMINFO as i32)?;
    m.add("CVAR_INIT", cvar_flags::CVAR_INIT as i32)?;
    m.add("CVAR_LATCH", cvar_flags::CVAR_LATCH as i32)?;
    m.add("CVAR_ROM", cvar_flags::CVAR_ROM as i32)?;
    m.add("CVAR_USER_CREATED", cvar_flags::CVAR_USER_CREATED as i32)?;
    m.add("CVAR_TEMP", cvar_flags::CVAR_TEMP as i32)?;
    m.add("CVAR_CHEAT", cvar_flags::CVAR_CHEAT as i32)?;
    m.add("CVAR_NORESTART", cvar_flags::CVAR_NORESTART as i32)?;

    // Privileges.
    m.add("PRIV_NONE", privileges_t::PRIV_NONE as i32)?;
    m.add("PRIV_MOD", privileges_t::PRIV_MOD as i32)?;
    m.add("PRIV_ADMIN", privileges_t::PRIV_ADMIN as i32)?;
    m.add("PRIV_ROOT", privileges_t::PRIV_ROOT as i32)?;
    m.add("PRIV_BANNED", privileges_t::PRIV_BANNED as i32)?;

    // Connection states.
    m.add("CS_FREE", clientState_t::CS_FREE as i32)?;
    m.add("CS_ZOMBIE", clientState_t::CS_ZOMBIE as i32)?;
    m.add("CS_CONNECTED", clientState_t::CS_CONNECTED as i32)?;
    m.add("CS_PRIMED", clientState_t::CS_PRIMED as i32)?;
    m.add("CS_ACTIVE", clientState_t::CS_ACTIVE as i32)?;

    // Teams.
    m.add("TEAM_FREE", team_t::TEAM_FREE as i32)?;
    m.add("TEAM_RED", team_t::TEAM_RED as i32)?;
    m.add("TEAM_BLUE", team_t::TEAM_BLUE as i32)?;
    m.add("TEAM_SPECTATOR", team_t::TEAM_SPECTATOR as i32)?;

    // Means of death.
    m.add("MOD_UNKNOWN", meansOfDeath_t::MOD_UNKNOWN as i32)?;
    m.add("MOD_SHOTGUN", meansOfDeath_t::MOD_SHOTGUN as i32)?;
    m.add("MOD_GAUNTLET", meansOfDeath_t::MOD_GAUNTLET as i32)?;
    m.add("MOD_MACHINEGUN", meansOfDeath_t::MOD_MACHINEGUN as i32)?;
    m.add("MOD_GRENADE", meansOfDeath_t::MOD_GRENADE as i32)?;
    m.add(
        "MOD_GRENADE_SPLASH",
        meansOfDeath_t::MOD_GRENADE_SPLASH as i32,
    )?;
    m.add("MOD_ROCKET", meansOfDeath_t::MOD_ROCKET as i32)?;
    m.add(
        "MOD_ROCKET_SPLASH",
        meansOfDeath_t::MOD_ROCKET_SPLASH as i32,
    )?;
    m.add("MOD_PLASMA", meansOfDeath_t::MOD_PLASMA as i32)?;
    m.add(
        "MOD_PLASMA_SPLASH",
        meansOfDeath_t::MOD_PLASMA_SPLASH as i32,
    )?;
    m.add("MOD_RAILGUN", meansOfDeath_t::MOD_RAILGUN as i32)?;
    m.add("MOD_LIGHTNING", meansOfDeath_t::MOD_LIGHTNING as i32)?;
    m.add("MOD_BFG", meansOfDeath_t::MOD_BFG as i32)?;
    m.add("MOD_BFG_SPLASH", meansOfDeath_t::MOD_BFG_SPLASH as i32)?;
    m.add("MOD_WATER", meansOfDeath_t::MOD_WATER as i32)?;
    m.add("MOD_SLIME", meansOfDeath_t::MOD_SLIME as i32)?;
    m.add("MOD_LAVA", meansOfDeath_t::MOD_LAVA as i32)?;
    m.add("MOD_CRUSH", meansOfDeath_t::MOD_CRUSH as i32)?;
    m.add("MOD_TELEFRAG", meansOfDeath_t::MOD_TELEFRAG as i32)?;
    m.add("MOD_FALLING", meansOfDeath_t::MOD_FALLING as i32)?;
    m.add("MOD_SUICIDE", meansOfDeath_t::MOD_SUICIDE as i32)?;
    m.add("MOD_TARGET_LASER", meansOfDeath_t::MOD_TARGET_LASER as i32)?;
    m.add("MOD_TRIGGER_HURT", meansOfDeath_t::MOD_TRIGGER_HURT as i32)?;
    m.add("MOD_NAIL", meansOfDeath_t::MOD_NAIL as i32)?;
    m.add("MOD_CHAINGUN", meansOfDeath_t::MOD_CHAINGUN as i32)?;
    m.add(
        "MOD_PROXIMITY_MINE",
        meansOfDeath_t::MOD_PROXIMITY_MINE as i32,
    )?;
    m.add("MOD_KAMIKAZE", meansOfDeath_t::MOD_KAMIKAZE as i32)?;
    m.add("MOD_JUICED", meansOfDeath_t::MOD_JUICED as i32)?;
    m.add("MOD_GRAPPLE", meansOfDeath_t::MOD_GRAPPLE as i32)?;
    m.add("MOD_SWITCH_TEAMS", meansOfDeath_t::MOD_SWITCH_TEAMS as i32)?;
    m.add("MOD_THAW", meansOfDeath_t::MOD_THAW as i32)?;
    m.add(
        "MOD_LIGHTNING_DISCHARGE",
        meansOfDeath_t::MOD_LIGHTNING_DISCHARGE as i32,
    )?;
    m.add("MOD_HMG", meansOfDeath_t::MOD_HMG as i32)?;
    m.add(
        "MOD_RAILGUN_HEADSHOT",
        meansOfDeath_t::MOD_RAILGUN_HEADSHOT as i32,
    )?;

    m.add("DAMAGE_RADIUS", DAMAGE_RADIUS as i32)?;
    m.add("DAMAGE_NO_ARMOR", DAMAGE_NO_ARMOR as i32)?;
    m.add("DAMAGE_NO_KNOCKBACK", DAMAGE_NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_NO_PROTECTION", DAMAGE_NO_PROTECTION as i32)?;
    m.add(
        "DAMAGE_NO_TEAM_PROTECTION",
        DAMAGE_NO_TEAM_PROTECTION as i32,
    )?;

    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;
    m.add_class::<Vector3>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Flight>()?;

    Ok(())
}

pub(crate) static PYMINQLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn pyminqlx_is_initialized() -> bool {
    PYMINQLX_INITIALIZED.load(Ordering::SeqCst)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum PythonInitializationError {
    MainScriptError,
    #[cfg_attr(test, allow(dead_code))]
    AlreadyInitialized,
    NotInitializedError,
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyminqlx_initialize() -> Result<(), PythonInitializationError> {
    if pyminqlx_is_initialized() {
        error!(target: "shinqlx", "pyminqlx_initialize was called while already initialized");
        return Err(PythonInitializationError::AlreadyInitialized);
    }

    debug!(target: "shinqlx", "Initializing Python...");
    append_to_inittab!(pyminqlx_module);
    prepare_freethreaded_python();
    match Python::with_gil(|py| {
        let minqlx_module = py.import("minqlx")?;
        minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(e) => {
            error!(target: "shinqlx", "{:?}", e);
            error!(target: "shinqlx", "loader sequence returned an error. Did you modify the loader?");
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(_) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            debug!(target: "shinqlx", "Python initialized!");
            Ok(())
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyminqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyminqlx_is_initialized() {
        error!(target: "shinqlx", "pyminqlx_finalize was called before being initialized");
        return Err(PythonInitializationError::NotInitializedError);
    }

    [
        &CLIENT_COMMAND_HANDLER,
        &SERVER_COMMAND_HANDLER,
        &FRAME_HANDLER,
        &PLAYER_CONNECT_HANDLER,
        &PLAYER_LOADED_HANDLER,
        &PLAYER_DISCONNECT_HANDLER,
        &CUSTOM_COMMAND_HANDLER,
        &NEW_GAME_HANDLER,
        &SET_CONFIGSTRING_HANDLER,
        &RCON_HANDLER,
        &CONSOLE_PRINT_HANDLER,
        &PLAYER_SPAWN_HANDLER,
        &KAMIKAZE_USE_HANDLER,
        &KAMIKAZE_EXPLODE_HANDLER,
        &DAMAGE_HANDLER,
    ]
    .into_iter()
    .for_each(|handler_lock| handler_lock.store(None));

    match Python::with_gil(|py| {
        let importlib_module = py.import("importlib")?;
        let minqlx_module = py.import("minqlx")?;
        let new_minqlx_module = importlib_module.call_method1("reload", (minqlx_module,))?;
        new_minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(_) => {
            PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(()) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
pub(crate) mod python {
    use crate::pyminqlx::PythonInitializationError;

    pub(crate) fn rcon_dispatcher<T>(_cmd: T)
    where
        T: AsRef<str> + 'static,
    {
    }

    pub(crate) fn client_command_dispatcher(_client_id: i32, _cmd: String) -> Option<String> {
        None
    }
    pub(crate) fn server_command_dispatcher(
        _client_id: Option<i32>,
        _cmd: String,
    ) -> Option<String> {
        None
    }
    pub(crate) fn client_loaded_dispatcher(_client_id: i32) {}

    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: String) -> Option<String> {
        None
    }

    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: String) {}

    pub(crate) fn console_print_dispatcher(_msg: String) -> Option<String> {
        None
    }

    pub(crate) fn new_game_dispatcher(_restart: bool) {}

    pub(crate) fn frame_dispatcher() {}

    pub(crate) fn client_connect_dispatcher(_client_id: i32, _is_bot: bool) -> Option<String> {
        None
    }

    pub(crate) fn client_spawn_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_use_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_explode_dispatcher(_client_id: i32, _is_used_on_demand: bool) {}

    pub(crate) fn damage_dispatcher(
        _target_client_id: i32,
        _attacker_client_id: Option<i32>,
        _damage: i32,
        _dflags: i32,
        _means_of_death: i32,
    ) {
    }

    pub(crate) fn pyminqlx_is_initialized() -> bool {
        false
    }

    pub(crate) fn pyminqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }

    pub(crate) fn pyminqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}
