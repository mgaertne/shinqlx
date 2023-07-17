use crate::commands::cmd_py_command;
use crate::hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
use crate::pyminqlx::PythonPriorities::{PRI_HIGH, PRI_HIGHEST, PRI_LOW, PRI_LOWEST, PRI_NORMAL};
use crate::pyminqlx::PythonReturnCodes::{
    RET_NONE, RET_STOP, RET_STOP_ALL, RET_STOP_EVENT, RET_USAGE,
};
use crate::quake_types::clientState_t::{CS_ACTIVE, CS_CONNECTED, CS_FREE, CS_PRIMED, CS_ZOMBIE};
use crate::quake_types::meansOfDeath_t::{
    MOD_BFG, MOD_BFG_SPLASH, MOD_CHAINGUN, MOD_CRUSH, MOD_FALLING, MOD_GAUNTLET, MOD_GRAPPLE,
    MOD_GRENADE, MOD_GRENADE_SPLASH, MOD_HMG, MOD_JUICED, MOD_KAMIKAZE, MOD_LAVA, MOD_LIGHTNING,
    MOD_LIGHTNING_DISCHARGE, MOD_MACHINEGUN, MOD_NAIL, MOD_PLASMA, MOD_PLASMA_SPLASH,
    MOD_PROXIMITY_MINE, MOD_RAILGUN, MOD_RAILGUN_HEADSHOT, MOD_ROCKET, MOD_ROCKET_SPLASH,
    MOD_SHOTGUN, MOD_SLIME, MOD_SUICIDE, MOD_SWITCH_TEAMS, MOD_TARGET_LASER, MOD_TELEFRAG,
    MOD_THAW, MOD_TRIGGER_HURT, MOD_UNKNOWN, MOD_WATER,
};
use crate::quake_types::privileges_t::{PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT};
use crate::quake_types::team_t::{TEAM_BLUE, TEAM_FREE, TEAM_RED, TEAM_SPECTATOR};
use crate::quake_types::{
    DAMAGE_NO_ARMOR, DAMAGE_NO_KNOCKBACK, DAMAGE_NO_PROTECTION, DAMAGE_NO_TEAM_PROTECTION,
    DAMAGE_RADIUS, MAX_CONFIGSTRINGS, MAX_GENTITIES,
};
use crate::{PyMinqlx_InitStatus_t, MAIN_ENGINE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use crate::client::Client;
use crate::current_level::CurrentLevel;
use crate::game_client::GameClient;
use crate::game_entity::GameEntity;
use crate::game_item::GameItem;
use crate::quake_live_engine::{
    AddCommand, ComPrintf, ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SendServerCommand,
    SetCVarForced, SetCVarLimit,
};
use crate::quake_types::cvar_flags::{
    CVAR_ARCHIVE, CVAR_CHEAT, CVAR_INIT, CVAR_LATCH, CVAR_NORESTART, CVAR_ROM, CVAR_SERVERINFO,
    CVAR_SYSTEMINFO, CVAR_TEMP, CVAR_USERINFO, CVAR_USER_CREATED,
};
use crate::quake_types::entityType_t::ET_ITEM;
use crate::PyMinqlx_InitStatus_t::{
    PYM_ALREADY_INITIALIZED, PYM_MAIN_SCRIPT_ERROR, PYM_NOT_INITIALIZED_ERROR, PYM_PY_INIT_ERROR,
    PYM_SUCCESS,
};
use crate::ALLOW_FREE_CLIENT;
use pyo3::append_to_inittab;
use pyo3::exceptions::{PyEnvironmentError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::prepare_freethreaded_python;
use pyo3::types::PyTuple;

pub(crate) fn client_command_dispatcher(client_id: i32, cmd: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(cmd.into());
    }

    let Ok(client_command_lock) = CLIENT_COMMAND_HANDLER.try_read() else {
        return Some(cmd.into());
    };

    let Some(ref client_command_handler) = *client_command_lock else {
        return Some(cmd.into());
    };

    Python::with_gil(
        |py| match client_command_handler.call1(py, (client_id, cmd)) {
            Err(_) => {
                dbg!("client_command_handler returned an error.\n");
                Some(cmd.into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn server_command_dispatcher(client_id: Option<i32>, cmd: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(cmd.into());
    }

    let Ok(server_command_lock) = SERVER_COMMAND_HANDLER.try_read() else {
        return Some(cmd.into());
    };
    let Some(ref server_command_handler) = *server_command_lock else {
        return Some(cmd.into());
    };

    Python::with_gil(
        |py| match server_command_handler.call1(py, (client_id.unwrap_or(-1), cmd)) {
            Err(_) => {
                dbg!("server_command_handler returned an error.\n");
                Some(cmd.into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn frame_dispatcher() {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(frame_handler_lock) = FRAME_HANDLER.try_read() else {
        return;
    };

    if let Some(ref frame_handler) = *frame_handler_lock {
        Python::with_gil(|py| {
            let result = frame_handler.call0(py);
            if result.is_err() {
                dbg!("frame_handler returned an error.\n");
            }
        });
    }
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return None;
    }

    let Ok(client_connect_lock) = PLAYER_CONNECT_HANDLER.try_read() else {
        return None;
    };

    let Some(ref client_connect_handler) = *client_connect_lock else {
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

pub(crate) fn client_disconnect_dispatcher(client_id: i32, reason: &str) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(client_disconnect_lock) = PLAYER_DISCONNECT_HANDLER.try_read() else {
        return;
    };

    let Some(ref client_disconnect_handler) = *client_disconnect_lock else {
        return;
    };

    ALLOW_FREE_CLIENT.store(client_id, Ordering::Relaxed);
    Python::with_gil(|py| {
        let result = client_disconnect_handler.call1(py, (client_id, reason));
        if result.is_err() {
            dbg!("client_disconnect_handler returned an error.\n");
        }
    });
    ALLOW_FREE_CLIENT.store(-1, Ordering::Relaxed);
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(client_loaded_lock) = PLAYER_LOADED_HANDLER.try_read() else {
        return;
    };

    if let Some(ref client_loaded_handler) = *client_loaded_lock {
        Python::with_gil(|py| {
            let returned_value = client_loaded_handler.call1(py, (client_id,));
            if returned_value.is_err() {
                dbg!("client_loaded_handler returned an error.\n");
            }
        });
    }
}

pub(crate) fn new_game_dispatcher(restart: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(new_game_lock) = NEW_GAME_HANDLER.try_read() else {
        return;
    };

    if let Some(ref new_game_handler) = *new_game_lock {
        Python::with_gil(|py| {
            let result = new_game_handler.call1(py, (restart,));
            if result.is_err() {
                dbg!("new_game_handler returned an error.\n");
            }
        });
    };
}

pub(crate) fn set_configstring_dispatcher(index: u32, value: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(value.into());
    }

    let Ok(set_configstring_lock) = SET_CONFIGSTRING_HANDLER.try_read() else {
        return Some(value.into());
    };

    let Some(ref set_configstring_handler) = *set_configstring_lock else {
        return Some(value.into());
    };

    Python::with_gil(
        |py| match set_configstring_handler.call1(py, (index, value)) {
            Err(_) => {
                dbg!("set_configstring_handler returned an error.\n");
                Some(value.into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(value.into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(value.into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn rcon_dispatcher(cmd: &str) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(rcon_lock) = RCON_HANDLER.try_read() else {
        return;
    };

    if let Some(ref rcon_handler) = *rcon_lock {
        Python::with_gil(|py| {
            let result = rcon_handler.call1(py, (cmd,));
            if result.is_err() {
                dbg!("rcon_handler returned an error.\n");
            }
        });
    }
}

pub(crate) fn console_print_dispatcher(text: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(text.into());
    }

    let Ok(console_print_lock) = CONSOLE_PRINT_HANDLER.try_read() else {
        return Some(text.into());
    };

    let Some(ref console_print_handler) = *console_print_lock else {
        return Some(text.into());
    };

    Python::with_gil(|py| match console_print_handler.call1(py, (text,)) {
        Err(_) => {
            dbg!("console_print_handler returned an error.\n");
            Some(text.into())
        }
        Ok(returned) => match returned.extract::<String>(py) {
            Err(_) => match returned.extract::<bool>(py) {
                Err(_) => Some(text.into()),
                Ok(result_bool) => {
                    if !result_bool {
                        None
                    } else {
                        Some(text.into())
                    }
                }
            },
            Ok(result_string) => Some(result_string),
        },
    })
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(client_spawn_lock) = PLAYER_SPAWN_HANDLER.try_read() else {
        return;
    };

    if let Some(ref client_spawn_handler) = *client_spawn_lock {
        Python::with_gil(|py| {
            let result = client_spawn_handler.call1(py, (client_id,));
            if result.is_err() {
                dbg!("client_spawn_handler returned an error.\n");
            }
        });
    }
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(kamikaze_use_lock) = KAMIKAZE_USE_HANDLER.try_read() else {
        return;
    };

    if let Some(ref kamikaze_use_handler) = *kamikaze_use_lock {
        Python::with_gil(|py| {
            let result = kamikaze_use_handler.call1(py, (client_id,));
            if result.is_err() {
                dbg!("kamikaze_use_handler returned an error.\n");
            }
        });
    }
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Ok(kamikaze_explode_lock) = KAMIKAZE_EXPLODE_HANDLER.try_read() else {
        return;
    };

    if let Some(ref kamikaze_explode_handler) = *kamikaze_explode_lock {
        Python::with_gil(|py| {
            let result = kamikaze_explode_handler.call1(py, (client_id, is_used_on_demand));
            if result.is_err() {
                dbg!("kamikaze_explode_handler returned an error.\n");
            }
        });
    }
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

    let Ok(damage_lock) = DAMAGE_HANDLER.try_read() else {
        return;
    };

    if let Some(ref damage_handler) = *damage_lock {
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
                dbg!("damage_handler returned an error.\n");
            }
        });
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
}

impl TryFrom<i32> for PlayerInfo {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        let game_entity_result = GameEntity::try_from(client_id);
        match game_entity_result {
            Err(_) => Ok(PlayerInfo {
                client_id,
                name: Default::default(),
                connection_state: CS_FREE as i32,
                userinfo: Default::default(),
                steam_id: 0,
                team: TEAM_SPECTATOR as i32,
                privileges: -1,
            }),
            Ok(game_entity) => {
                let Ok(client) = Client::try_from(client_id) else {
                    return Ok(PlayerInfo {
                        client_id,
                        name: game_entity.get_player_name(),
                        connection_state: CS_FREE as i32,
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }
        if let Ok(client) = Client::try_from(client_id) {
            let allowed_free_client_id = ALLOW_FREE_CLIENT.load(Ordering::Relaxed);
            if allowed_free_client_id != client_id && client.get_state() == CS_FREE {
                #[cfg(debug_assertions)]
                println!(
                    "WARNING: get_player_info called for CS_FREE client {}.",
                    client_id
                );
                return Ok(None);
            }
        }
        Ok(PlayerInfo::try_from(client_id).ok())
    })
}

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
fn get_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        let result: Vec<Option<PlayerInfo>> = (0..maxclients)
            .filter_map(|client_id| {
                Client::try_from(client_id).map_or_else(
                    |_| None,
                    |client| match client.get_state() {
                        CS_FREE => None,
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match Client::try_from(client_id) {
            Err(_) => Ok(None),
            Ok(client) => {
                let allowed_free_client_id = ALLOW_FREE_CLIENT.load(Ordering::Relaxed);
                if allowed_free_client_id != client_id && client.get_state() == CS_FREE {
                    Ok(None)
                } else {
                    Ok(Some(client.get_user_info()))
                }
            }
        }
    })
}

/// Sends a server command to either one specific client or all the clients.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
fn send_server_command(client_id: Option<i32>, cmd: &str) -> PyResult<bool> {
    match client_id {
        None => {
            shinqlx_send_server_command(None, cmd);
            Ok(true)
        }
        Some(actual_client_id) => {
            let Ok(main_engine_guard) = MAIN_ENGINE.try_read() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not accessible",
                ));
            };

            let Some(ref main_engine) = *main_engine_guard else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let maxclients = main_engine.get_max_clients();
            if !(0..maxclients).contains(&actual_client_id) {
                return Err(PyValueError::new_err(format!(
                    "client_id needs to be a number from 0 to {}, or None.",
                    maxclients - 1
                )));
            }
            match Client::try_from(actual_client_id) {
                Err(_) => Ok(false),
                Ok(client) => {
                    if client.get_state() != CS_ACTIVE {
                        Ok(false)
                    } else {
                        shinqlx_send_server_command(Some(client), cmd);
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}, or None.",
                maxclients - 1
            )));
        }

        match Client::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(client) => {
                if [CS_FREE, CS_ZOMBIE].contains(&client.get_state()) {
                    Ok(false)
                } else {
                    shinqlx_execute_client_command(Some(client), cmd, true);
                    Ok(true)
                }
            }
        }
    })
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
fn console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}, or None.",
                maxclients - 1
            )));
        }

        match Client::try_from(client_id) {
            Err(_) => Err(PyValueError::new_err(
                "client_id must be None or the ID of an active player.",
            )),
            Ok(mut client) => {
                if client.get_state() != CS_ACTIVE {
                    return Err(PyValueError::new_err(
                        "client_id must be None or the ID of an active player.",
                    ));
                }
                let reason_str = if reason.unwrap_or("was kicked.").is_empty() {
                    "was kicked."
                } else {
                    reason.unwrap_or("was kicked.")
                };
                shinqlx_drop_client(&mut client, reason_str);
                Ok(())
            }
        }
    })
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
fn console_print(py: Python<'_>, text: &str) {
    py.allow_threads(|| {
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

    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_configstring(config_id))
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
    py.allow_threads(|| {
        shinqlx_set_configstring(config_id, value);
    });
    Ok(())
}

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
fn force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
    let current_level = CurrentLevel::default();
    let vote_time = current_level.get_vote_time();
    if vote_time.is_none() {
        return Ok(false);
    }

    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        (0..maxclients)
            .filter(|i| Client::try_from(*i).is_ok_and(|client| client.get_state() == CS_ACTIVE))
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_state(pass));

        Ok(true)
    })
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
fn add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.add_command(command, cmd_py_command);

        Ok(())
    })
}

static CLIENT_COMMAND_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static SERVER_COMMAND_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static FRAME_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static PLAYER_CONNECT_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static PLAYER_LOADED_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static PLAYER_DISCONNECT_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
pub(crate) static CUSTOM_COMMAND_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static NEW_GAME_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static SET_CONFIGSTRING_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static RCON_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static CONSOLE_PRINT_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static PLAYER_SPAWN_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static KAMIKAZE_USE_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static KAMIKAZE_EXPLODE_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);
static DAMAGE_HANDLER: RwLock<Option<Py<PyAny>>> = RwLock::new(None);

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

    let Ok(mut guard) = handler_lock.write() else {
        return Err(PyEnvironmentError::new_err("handler lock was poisoned."));
    };

    *guard = handler;

    Ok(())
}

#[pyclass]
struct Vector3Iter {
    iter: std::vec::IntoIter<i32>,
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
#[derive(PartialEq, Debug, Clone, Copy, Default)]
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

    fn __str__(&self) -> String {
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
pub(crate) mod vector3_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use hamcrest::prelude::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    pub(crate) fn vector3_tuple_test(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let minqlx_module = py.import("_minqlx").unwrap();
            let vector3 = minqlx_module.getattr("Vector3").unwrap();
            let tuple = py.import("builtins").unwrap().getattr("tuple").unwrap();
            assert_that!(vector3.is_instance(tuple.get_type()).unwrap(), is(true));
        });
    }

    #[rstest]
    pub(crate) fn vector3_can_be_created_from_python(_pyminqlx_setup: ()) {
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
#[derive(PartialEq, Debug, Clone, Copy)]
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

    fn __str__(&self) -> String {
        format!("Weapons(g={}, mg={}, sg={}, gl={}, rl={}, lg={}, rg={}, pg={}, bfg={}, gh={}, ng={}, pl={}, cg={}, hmg={}, hands={})",
        self.0, self.1, self.2, self.3, self.4, self.5, self.5, self.7, self.8, self.9, self.10, self.11, self.12, self.13, self.14)
    }
}

#[cfg(test)]
pub(crate) mod weapons_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    pub(crate) fn weapons_can_be_created_from_python(_pyminqlx_setup: ()) {
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
pub(crate) mod ammo_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    pub(crate) fn ammo_can_be_created_from_python(_pyminqlx_setup: ()) {
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
#[derive(PartialEq, Debug, Clone, Copy)]
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

    fn __str__(&self) -> String {
        format!("Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5)
    }
}

#[cfg(test)]
pub(crate) mod powerups_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    pub(crate) fn powerups_can_be_created_from_python(_pyminqlx_setup: ()) {
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
#[derive(PartialEq, Debug, Clone, Copy)]
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

    fn __str__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }
}

#[cfg(test)]
pub(crate) mod flight_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    pub(crate) fn flight_can_be_created_from_python(_pyminqlx_setup: ()) {
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(None),
            Ok(game_entity) => {
                if game_entity.get_game_client().is_err() {
                    return Ok(None);
                }
                Ok(Some(PlayerState::from(game_entity)))
            }
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(None),
            Ok(game_entity) => Ok(Some(PlayerStats::from(
                game_entity.get_game_client().unwrap(),
            ))),
        }
    })
}

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
fn set_position(py: Python<'_>, client_id: i32, position: Vector3) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut mutable_client = game_entity.get_game_client().unwrap();
                mutable_client.set_position((
                    position.0 as f32,
                    position.1 as f32,
                    position.2 as f32,
                ));
                Ok(true)
            }
        }
    })
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
fn set_velocity(py: Python<'_>, client_id: i32, velocity: Vector3) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut mutable_client = game_entity.get_game_client().unwrap();
                mutable_client.set_velocity((
                    velocity.0 as f32,
                    velocity.1 as f32,
                    velocity.2 as f32,
                ));
                Ok(true)
            }
        }
    })
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
fn noclip(py: Python<'_>, client_id: i32, activate: bool) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
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
        }
    })
}

/// Sets a player's health.
#[pyfunction]
#[pyo3(name = "set_health")]
fn set_health(py: Python<'_>, client_id: i32, health: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_entity = game_entity;
                game_entity.set_health(health);
                Ok(true)
            }
        }
    })
}

/// Sets a player's armor.
#[pyfunction]
#[pyo3(name = "set_armor")]
fn set_armor(py: Python<'_>, client_id: i32, armor: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_armor(armor);
                Ok(true)
            }
        }
    })
}

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
fn set_weapons(py: Python<'_>, client_id: i32, weapons: Weapons) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_weapons(weapons.into());
                Ok(true)
            }
        }
    })
}

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
fn set_weapon(py: Python<'_>, client_id: i32, weapon: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
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

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_weapon(weapon);
                Ok(true)
            }
        }
    })
}

/// Sets a player's ammo.
#[pyfunction]
#[pyo3(name = "set_ammo")]
fn set_ammo(py: Python<'_>, client_id: i32, ammos: Weapons) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_ammos(ammos.into());
                Ok(true)
            }
        }
    })
}

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
fn set_powerups(py: Python<'_>, client_id: i32, powerups: Powerups) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();

        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_powerups(powerups.into());
                Ok(true)
            }
        }
    })
}

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
fn set_holdable(py: Python<'_>, client_id: i32, holdable: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                let ql_holdable = Holdable::from(holdable);
                game_client.set_holdable(ql_holdable);
                Ok(true)
            }
        }
    })
}

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
fn drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
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
        }
    })
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
fn set_flight(py: Python<'_>, client_id: i32, flight: Flight) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_flight::<[i32; 4]>(flight.into());
                Ok(true)
            }
        }
    })
}

/// Makes player invulnerable for limited time.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
fn set_invulnerability(py: Python<'_>, client_id: i32, time: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_invulnerability(time);
                Ok(true)
            }
        }
    })
}

/// Sets a player's score.
#[pyfunction]
#[pyo3(name = "set_score")]
fn set_score(py: Python<'_>, client_id: i32, score: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_score(score);
                Ok(true)
            }
        }
    })
}

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
fn callvote(py: Python<'_>, vote: &str, vote_disp: &str, vote_time: Option<i32>) {
    py.allow_threads(|| {
        let mut current_level = CurrentLevel::default();
        current_level.callvote(vote, vote_disp, vote_time);
    })
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
fn allow_single_player(py: Python<'_>, allow: bool) {
    py.allow_threads(|| {
        let mut current_level = CurrentLevel::default();
        current_level.set_training_map(allow);
    })
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
fn player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
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
        }
    })
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
fn set_privileges(py: Python<'_>, client_id: i32, privileges: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
            Err(_) => Ok(false),
            Ok(game_entity) => match game_entity.get_game_client() {
                Err(_) => Ok(false),
                Ok(game_client) => {
                    let mut game_client = game_client;
                    game_client.set_privileges(privileges);
                    Ok(true)
                }
            },
        }
    })
}

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
fn destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        let mut in_use_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i).ok())
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
    });
    Ok(true)
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

    py.allow_threads(|| {
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
            .filter_map(|i| GameEntity::try_from(i).ok())
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
    py.allow_threads(|| {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}.",
                maxclients - 1
            )));
        }

        match GameEntity::try_from(client_id) {
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
        }
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

    if let Ok(item_classname) = item.extract::<String>() {
        return (1..GameItem::get_num_items())
            .filter(|i| {
                let game_item = GameItem::try_from(*i);
                game_item.is_ok() && game_item.unwrap().get_classname() == item_classname
            })
            .take(1)
            .next()
            .ok_or(PyValueError::new_err(format!(
                "invalid item classname: {}",
                item_classname
            )));
    }

    Err(PyValueError::new_err(
        "item needs to be type of int or string.",
    ))
}

/// Replaces target entity's item with specified one.
#[allow(unused_variables)]
#[pyfunction]
#[pyo3(name = "replace_items")]
#[pyo3(signature = (item1, item2))]
fn replace_items(py: Python<'_>, item1: Py<PyAny>, item2: Py<PyAny>) -> PyResult<bool> {
    let item2_id_result = determine_item_id(item2.as_ref(py));
    if item2_id_result.is_err() {
        return Err(item2_id_result.err().unwrap());
    }
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed
    let item2_id = item2_id_result.unwrap();

    if let Ok(item1_id) = item1.extract::<i32>(py) {
        // replacing item by entity_id

        // entity_id checking
        if item1_id < 0 || item1_id >= MAX_GENTITIES as i32 {
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
            )));
        }

        match GameEntity::try_from(item1_id) {
            Err(_) => return Err(PyValueError::new_err("game entity does not exist")),
            Ok(game_entity) => {
                if !game_entity.in_use() {
                    return Err(PyValueError::new_err(format!(
                        "entity #{} is not in use.",
                        item1_id
                    )));
                }
                if !game_entity.is_game_item(ET_ITEM) {
                    return Err(PyValueError::new_err(format!(
                        "entity #{} is not item. Cannot replace it",
                        item1_id
                    )));
                }
                let mut mut_game_entity = game_entity;
                mut_game_entity.replace_item(item2_id);
            }
        }
        return Ok(true);
    }

    if let Ok(item1_classname) = item1.extract::<String>(py) {
        let matching_item1_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use()
                    && game_entity.is_game_item(ET_ITEM)
                    && game_entity.get_classname() == item1_classname
            })
            .collect();
        let item_found = !matching_item1_entities.is_empty();
        matching_item1_entities
            .into_iter()
            .for_each(|mut game_entity| game_entity.replace_item(item2_id));
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
    py.allow_threads(|| {
        let formatted_items: Vec<String> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_game_item(ET_ITEM))
            .map(|game_entity| {
                format!(
                    "{} {}",
                    game_entity.get_entity_id(),
                    game_entity.get_classname()
                )
            })
            .collect();
        let mut str_length = 0;
        let printed_items: Vec<String> = formatted_items
            .iter()
            .take_while(|&item| {
                str_length += item.len();
                str_length < 1024
            })
            .map(|item| item.to_string())
            .collect();

        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not accessible",
            ));
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if printed_items.is_empty() {
            main_engine.send_server_command(None, "print \"No items found in the map\n\"");
            return Ok(());
        }
        main_engine.send_server_command(
            None,
            format!("print \"{}\n\"", printed_items.join("\n")).as_str(),
        );

        let remaining_items: Vec<String> = formatted_items
            .iter()
            .skip(printed_items.len())
            .map(|item| item.to_string())
            .collect();

        if !remaining_items.is_empty() {
            main_engine
                .send_server_command(None, "print \"Check server console for other items\n\"\n");
            remaining_items
                .into_iter()
                .for_each(|item| main_engine.com_printf(item.as_str()));
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

    py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_respawning_weapon())
            .for_each(|mut game_entity| game_entity.set_respawn_time(respawn_time));
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

    py.allow_threads(|| {
        if let Ok(entity) = GameEntity::try_from(entity_id) {
            Ok(entity.get_targetting_entity_ids())
        } else {
            Ok(vec![])
        }
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

#[allow(non_camel_case_types, dead_code)]
enum cvar_flags {
    CVAR_ARCHIVE = 1,
    CVAR_USERINFO = 2,
    CVAR_SERVERINFO = 4,
    CVAR_SYSTEMINFO = 8,
    CVAR_INIT = 16,
    CVAR_LATCH = 32,
    CVAR_ROM = 64,
    CVAR_USER_CREATED = 128,
    CVAR_TEMP = 256,
    CVAR_CHEAT = 512,
    CVAR_NORESTART = 1024,
    CVAR_UNKOWN1 = 2048,
    CVAR_UNKOWN2 = 4096,
    CVAR_UNKOWN3 = 8192,
    CVAR_UNKOWN4 = 16384,
    CVAR_UNKOWN5 = 32768,
    CVAR_UNKOWN6 = 65536,
    CVAR_UNKOWN7 = 131072,
    CVAR_UNKOWN8 = 262144,
    CVAR_UNKOWN9 = 524288,
    CVAR_UNKOWN10 = 1048576,
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

    let shinqlx_version = format!(
        "\"v{}-{}\"",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME")
    );
    m.add("__version__", shinqlx_version.as_str())?;
    m.add("DEBUG", cfg!(debug_assertions))?;

    // Set a bunch of constants. We set them here because if you define functions in Python that use module
    // constants as keyword defaults, we have to always make sure they're exported first, and fuck that.
    m.add("RET_NONE", RET_NONE as i32)?;
    m.add("RET_STOP", RET_STOP as i32)?;
    m.add("RET_STOP_EVENT", RET_STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", RET_STOP_ALL as i32)?;
    m.add("RET_USAGE", RET_USAGE as i32)?;
    m.add("PRI_HIGHEST", PRI_HIGHEST as i32)?;
    m.add("PRI_HIGH", PRI_HIGH as i32)?;
    m.add("PRI_NORMAL", PRI_NORMAL as i32)?;
    m.add("PRI_LOW", PRI_LOW as i32)?;
    m.add("PRI_LOWEST", PRI_LOWEST as i32)?;

    // Cvar flags.
    m.add("CVAR_ARCHIVE", CVAR_ARCHIVE as i32)?;
    m.add("CVAR_USERINFO", CVAR_USERINFO as i32)?;
    m.add("CVAR_SERVERINFO", CVAR_SERVERINFO as i32)?;
    m.add("CVAR_SYSTEMINFO", CVAR_SYSTEMINFO as i32)?;
    m.add("CVAR_INIT", CVAR_INIT as i32)?;
    m.add("CVAR_LATCH", CVAR_LATCH as i32)?;
    m.add("CVAR_ROM", CVAR_ROM as i32)?;
    m.add("CVAR_USER_CREATED", CVAR_USER_CREATED as i32)?;
    m.add("CVAR_TEMP", CVAR_TEMP as i32)?;
    m.add("CVAR_CHEAT", CVAR_CHEAT as i32)?;
    m.add("CVAR_NORESTART", CVAR_NORESTART as i32)?;

    // Privileges.
    m.add("PRIV_NONE", PRIV_NONE as i32)?;
    m.add("PRIV_MOD", PRIV_MOD as i32)?;
    m.add("PRIV_ADMIN", PRIV_ADMIN as i32)?;
    m.add("PRIV_ROOT", PRIV_ROOT as i32)?;
    m.add("PRIV_BANNED", PRIV_BANNED as i32)?;

    // Connection states.
    m.add("CS_FREE", CS_FREE as i32)?;
    m.add("CS_ZOMBIE", CS_ZOMBIE as i32)?;
    m.add("CS_CONNECTED", CS_CONNECTED as i32)?;
    m.add("CS_PRIMED", CS_PRIMED as i32)?;
    m.add("CS_ACTIVE", CS_ACTIVE as i32)?;

    // Teams.
    m.add("TEAM_FREE", TEAM_FREE as i32)?;
    m.add("TEAM_RED", TEAM_RED as i32)?;
    m.add("TEAM_BLUE", TEAM_BLUE as i32)?;
    m.add("TEAM_SPECTATOR", TEAM_SPECTATOR as i32)?;

    // Means of death.
    m.add("MOD_UNKNOWN", MOD_UNKNOWN as i32)?;
    m.add("MOD_SHOTGUN", MOD_SHOTGUN as i32)?;
    m.add("MOD_GAUNTLET", MOD_GAUNTLET as i32)?;
    m.add("MOD_MACHINEGUN", MOD_MACHINEGUN as i32)?;
    m.add("MOD_GRENADE", MOD_GRENADE as i32)?;
    m.add("MOD_GRENADE_SPLASH", MOD_GRENADE_SPLASH as i32)?;
    m.add("MOD_ROCKET", MOD_ROCKET as i32)?;
    m.add("MOD_ROCKET_SPLASH", MOD_ROCKET_SPLASH as i32)?;
    m.add("MOD_PLASMA", MOD_PLASMA as i32)?;
    m.add("MOD_PLASMA_SPLASH", MOD_PLASMA_SPLASH as i32)?;
    m.add("MOD_RAILGUN", MOD_RAILGUN as i32)?;
    m.add("MOD_LIGHTNING", MOD_LIGHTNING as i32)?;
    m.add("MOD_BFG", MOD_BFG as i32)?;
    m.add("MOD_BFG_SPLASH", MOD_BFG_SPLASH as i32)?;
    m.add("MOD_WATER", MOD_WATER as i32)?;
    m.add("MOD_SLIME", MOD_SLIME as i32)?;
    m.add("MOD_LAVA", MOD_LAVA as i32)?;
    m.add("MOD_CRUSH", MOD_CRUSH as i32)?;
    m.add("MOD_TELEFRAG", MOD_TELEFRAG as i32)?;
    m.add("MOD_FALLING", MOD_FALLING as i32)?;
    m.add("MOD_SUICIDE", MOD_SUICIDE as i32)?;
    m.add("MOD_TARGET_LASER", MOD_TARGET_LASER as i32)?;
    m.add("MOD_TRIGGER_HURT", MOD_TRIGGER_HURT as i32)?;
    m.add("MOD_NAIL", MOD_NAIL as i32)?;
    m.add("MOD_CHAINGUN", MOD_CHAINGUN as i32)?;
    m.add("MOD_PROXIMITY_MINE", MOD_PROXIMITY_MINE as i32)?;
    m.add("MOD_KAMIKAZE", MOD_KAMIKAZE as i32)?;
    m.add("MOD_JUICED", MOD_JUICED as i32)?;
    m.add("MOD_GRAPPLE", MOD_GRAPPLE as i32)?;
    m.add("MOD_SWITCH_TEAMS", MOD_SWITCH_TEAMS as i32)?;
    m.add("MOD_THAW", MOD_THAW as i32)?;
    m.add("MOD_LIGHTNING_DISCHARGE", MOD_LIGHTNING_DISCHARGE as i32)?;
    m.add("MOD_HMG", MOD_HMG as i32)?;
    m.add("MOD_RAILGUN_HEADSHOT", MOD_RAILGUN_HEADSHOT as i32)?;

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

pub(crate) fn pyminqlx_initialize() -> PyMinqlx_InitStatus_t {
    if pyminqlx_is_initialized() {
        #[cfg(debug_assertions)]
        println!("pyminqlx_initialize was called while already initialized");
        return PYM_ALREADY_INITIALIZED;
    }

    #[cfg(debug_assertions)]
    println!("Initializing Python...");
    append_to_inittab!(pyminqlx_module);
    prepare_freethreaded_python();
    match Python::with_gil(|py| {
        let minqlx_module = py.import("minqlx")?;
        minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(e) => {
            debug_println!(e);
            #[cfg(debug_assertions)]
            println!("loader sequence returned an error. Did you modify the loader?");
            PYM_MAIN_SCRIPT_ERROR
        }
        Ok(_) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            #[cfg(debug_assertions)]
            println!("Python initialized!");
            PYM_SUCCESS
        }
    }
}

pub(crate) fn pyminqlx_reload() -> PyMinqlx_InitStatus_t {
    if !pyminqlx_is_initialized() {
        #[cfg(debug_assertions)]
        println!("pyminqlx_finalize was called before being initialized");
        return PYM_NOT_INITIALIZED_ERROR;
    }

    let mut failures = 0;
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
    .for_each(|handler_lock| {
        match handler_lock.write() {
            Err(_) => failures += 1,
            Ok(mut guard) => *guard = None,
        };
    });

    if failures != 0 {
        return PYM_PY_INIT_ERROR;
    }

    match Python::with_gil(|py| {
        let importlib_module = py.import("importlib")?;
        let minqlx_module = py.import("minqlx")?;
        let new_minqlx_module = importlib_module.call_method1("reload", (minqlx_module,))?;
        new_minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(_) => {
            PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);
            PYM_MAIN_SCRIPT_ERROR
        }
        Ok(()) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            PYM_SUCCESS
        }
    }
}
