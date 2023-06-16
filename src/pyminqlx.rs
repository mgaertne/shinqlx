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
    gitem_t, DAMAGE_NO_ARMOR, DAMAGE_NO_KNOCKBACK, DAMAGE_NO_PROTECTION, DAMAGE_NO_TEAM_PROTECTION,
    DAMAGE_RADIUS, MAX_CONFIGSTRINGS, MAX_GENTITIES,
};
use crate::PyMinqlx_InitStatus_t;

use crate::quake_live_engine::{
    AddCommand, Client, ComPrintf, ConsoleCommand, CurrentLevel, FindCVar, GameClient, GameEntity,
    GetCVar, GetConfigstring, QuakeLiveEngine, SendServerCommand, SetCVarForced, SetCVarLimit,
};
use crate::quake_types::cvar_flags::{
    CVAR_ARCHIVE, CVAR_CHEAT, CVAR_INIT, CVAR_LATCH, CVAR_NORESTART, CVAR_ROM, CVAR_SERVERINFO,
    CVAR_SYSTEMINFO, CVAR_TEMP, CVAR_USERINFO, CVAR_USER_CREATED,
};
use crate::quake_types::entityType_t::ET_ITEM;
use crate::PyMinqlx_InitStatus_t::{
    PYM_ALREADY_INITIALIZED, PYM_MAIN_SCRIPT_ERROR, PYM_NOT_INITIALIZED_ERROR, PYM_SUCCESS,
};
use crate::{ALLOW_FREE_CLIENT, SV_MAXCLIENTS};
use pyo3::append_to_inittab;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::prepare_freethreaded_python;
use pyo3::types::PyList;
use pyo3::types::PyTuple;
use std::ffi::{c_int, CStr};

#[allow(dead_code)]
fn py_type_check(value: &PyAny, type_name: &str) -> bool {
    match value.get_type().name() {
        Err(_) => false,
        Ok(python_type_name) => python_type_name == type_name,
    }
}

#[allow(dead_code)]
fn py_extract_bool_value(value: &PyAny) -> Option<bool> {
    if py_type_check(value, "bool") {
        let extracted_bool: PyResult<bool> = value.extract();
        match extracted_bool {
            Err(_) => None,
            Ok(bool) => Some(bool),
        }
    } else {
        None
    }
}

fn py_extract_str_value(value: &PyAny) -> Option<String> {
    if py_type_check(value, "str") {
        let extracted_string: PyResult<String> = value.extract();
        match extracted_string {
            Err(_) => None,
            Ok(string) => Some(string),
        }
    } else {
        None
    }
}

fn py_extract_int_value(value: &PyAny) -> Option<i32> {
    if py_type_check(value, "int") {
        let extracted_int: PyResult<i32> = value.extract();
        match extracted_int {
            Err(_) => None,
            Ok(int) => Some(int),
        }
    } else {
        None
    }
}

pub(crate) fn client_command_dispatcher(client_id: i32, cmd: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(cmd.into());
    }

    let Some(client_command_handler) = (unsafe { CLIENT_COMMAND_HANDLER.as_ref() }) else {
        return Some(cmd.into());
    };

    Python::with_gil(
        |py| match client_command_handler.call1(py, (client_id, cmd)) {
            Err(_) => {
                dbg!("client_command_handler returned an error.\n");
                Some(cmd.into())
            }
            Ok(returned) => {
                if py_extract_bool_value(returned.as_ref(py))
                    .is_some_and(|extracted_bool| !extracted_bool)
                {
                    None
                } else if let Some(extracted_string) = py_extract_str_value(returned.as_ref(py)) {
                    Some(extracted_string)
                } else {
                    Some(cmd.into())
                }
            }
        },
    )
}

pub(crate) fn server_command_dispatcher(client_id: Option<i32>, cmd: &str) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return Some(cmd.into());
    }

    let Some(server_command_handler) = (unsafe { SERVER_COMMAND_HANDLER.as_ref() }) else {
        return Some(cmd.into());
    };

    Python::with_gil(
        |py| match server_command_handler.call1(py, (client_id.unwrap_or(-1), cmd)) {
            Err(_) => {
                dbg!("server_command_handler returned an error.\n");
                Some(cmd.into())
            }
            Ok(returned) => {
                if py_extract_bool_value(returned.as_ref(py))
                    .is_some_and(|extracted_bool| !extracted_bool)
                {
                    None
                } else if let Some(extracted_string) = py_extract_str_value(returned.as_ref(py)) {
                    Some(extracted_string)
                } else {
                    Some(cmd.into())
                }
            }
        },
    )
}

pub(crate) fn frame_dispatcher() {
    if !pyminqlx_is_initialized() {
        return;
    }

    if let Some(frame_handler) = unsafe { FRAME_HANDLER.as_ref() } {
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

    let Some(client_connect_handler) = (unsafe { PLAYER_CONNECT_HANDLER.as_ref() }) else {
        return None;
    };

    unsafe {
        ALLOW_FREE_CLIENT = client_id;
    }

    let result: Option<String> =
        Python::with_gil(
            |py| match client_connect_handler.call1(py, (client_id, is_bot)) {
                Err(_) => None,
                Ok(returned) => {
                    if py_extract_bool_value(returned.as_ref(py))
                        .is_some_and(|extracted_bool| !extracted_bool)
                    {
                        Some("You are banned from this server.".into())
                    } else {
                        py_extract_str_value(returned.as_ref(py))
                    }
                }
            },
        );

    unsafe {
        ALLOW_FREE_CLIENT = -1;
    }

    result
}

pub(crate) fn client_disconnect_dispatcher(client_id: i32, reason: &str) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(client_disconnect_handler) = (unsafe { PLAYER_DISCONNECT_HANDLER.as_ref() }) else { return; };
    unsafe {
        ALLOW_FREE_CLIENT = client_id;
    }
    Python::with_gil(|py| {
        let result = client_disconnect_handler.call1(py, (client_id, reason));
        if result.is_err() {
            dbg!("client_disconnect_handler returned an error.\n");
        }
    });
    unsafe {
        ALLOW_FREE_CLIENT = -1;
    }
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    if let Some(client_loaded_handler) = unsafe { PLAYER_LOADED_HANDLER.as_ref() } {
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

    if let Some(new_game_handler) = unsafe { NEW_GAME_HANDLER.as_ref() } {
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

    let Some(set_configstring_handler) = (unsafe { SET_CONFIGSTRING_HANDLER.as_ref() }) else {
        return Some(value.into())
    };
    Python::with_gil(
        |py| match set_configstring_handler.call1(py, (index, value)) {
            Err(_) => {
                dbg!("set_configstring_handler returned an error.\n");
                Some(value.into())
            }
            Ok(returned) => {
                if py_extract_bool_value(returned.as_ref(py))
                    .is_some_and(|extracted_bool| !extracted_bool)
                {
                    None
                } else if let Some(extracted_string) = py_extract_str_value(returned.as_ref(py)) {
                    Some(extracted_string)
                } else {
                    Some(value.into())
                }
            }
        },
    )
}

pub(crate) fn rcon_dispatcher(cmd: &str) {
    if !pyminqlx_is_initialized() {
        return;
    }

    if let Some(rcon_handler) = unsafe { RCON_HANDLER.as_ref() } {
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
    let Some(console_print_handler) = (unsafe { CONSOLE_PRINT_HANDLER.as_ref() }) else { return Some(text.into()); };
    Python::with_gil(|py| match console_print_handler.call1(py, (text,)) {
        Err(_) => {
            dbg!("console_print_handler returned an error.\n");
            Some(text.into())
        }
        Ok(returned) => {
            if py_extract_bool_value(returned.as_ref(py))
                .is_some_and(|extracted_bool| !extracted_bool)
            {
                None
            } else if let Some(extracted_string) = py_extract_str_value(returned.as_ref(py)) {
                Some(extracted_string)
            } else {
                Some(text.into())
            }
        }
    })
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }
    if let Some(client_spawn_handler) = unsafe { PLAYER_SPAWN_HANDLER.as_ref() } {
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
    if let Some(kamikaze_use_handler) = unsafe { KAMIKAZE_USE_HANDLER.as_ref() } {
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
    if let Some(kamikaze_explode_handler) = unsafe { KAMIKAZE_EXPLODE_HANDLER.as_ref() } {
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

    if let Some(damage_handler) = unsafe { DAMAGE_HANDLER.as_ref() } {
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
                if let Err(error) = returned_value {
                    dbg!(error);
                    dbg!(target_client_id);
                    dbg!(attacker_client_id);
                }
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
fn get_player_info(client_id: i32) -> PyResult<Option<PlayerInfo>> {
    let maxclients = unsafe { SV_MAXCLIENTS };
    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }
    if let Ok(client) = Client::try_from(client_id) {
        let allowed_free_client_id = unsafe { ALLOW_FREE_CLIENT };
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
}

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
fn get_players_info() -> PyResult<Vec<Option<PlayerInfo>>> {
    let mut result = Vec::new();
    let maxclients = unsafe { SV_MAXCLIENTS };
    for client_id in 0..maxclients {
        match Client::try_from(client_id) {
            Err(_) => result.push(None),
            Ok(client) => {
                if client.get_state() == CS_FREE {
                    result.push(None)
                } else {
                    result.push(PlayerInfo::try_from(client_id).ok())
                }
            }
        }
    }
    Ok(result)
}

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
fn get_userinfo(client_id: i32) -> PyResult<Option<String>> {
    let maxclients = unsafe { SV_MAXCLIENTS };
    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    match Client::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(client) => {
            let allowed_free_client_id = unsafe { ALLOW_FREE_CLIENT };
            if allowed_free_client_id != client_id && client.get_state() == CS_FREE {
                Ok(None)
            } else {
                Ok(Some(client.get_user_info()))
            }
        }
    }
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
            let maxclients = unsafe { SV_MAXCLIENTS };
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
fn client_command(client_id: i32, cmd: &str) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
fn console_command(cmd: &str) {
    QuakeLiveEngine::default().execute_console_command(cmd);
}

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
fn get_cvar(cvar: &str) -> PyResult<Option<String>> {
    match QuakeLiveEngine::default().find_cvar(cvar) {
        None => Ok(None),
        Some(cvar_result) => Ok(Some(cvar_result.get_string())),
    }
}

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
fn set_cvar(cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
    let quake_live_engine = QuakeLiveEngine::default();
    match quake_live_engine.find_cvar(cvar) {
        None => {
            quake_live_engine.get_cvar(cvar, value, flags);
            Ok(true)
        }
        Some(_) => {
            quake_live_engine.set_cvar_forced(
                cvar,
                value,
                flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
            );
            Ok(false)
        }
    }
}

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
fn set_cvar_limit(cvar: &str, value: &str, min: &str, max: &str, flags: Option<i32>) {
    QuakeLiveEngine::default().set_cvar_limit(cvar, value, min, max, flags);
}

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
fn kick(client_id: i32, reason: Option<&str>) -> PyResult<()> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
        Ok(client) => {
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
            shinqlx_drop_client(&client, reason_str);
            Ok(())
        }
    }
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
fn console_print(text: &str) {
    let formatted_string = format!("{}\n", text);
    shinqlx_com_printf(formatted_string.as_str());
}

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
fn get_configstring(config_id: u32) -> PyResult<String> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }
    Ok(QuakeLiveEngine::default().get_configstring(config_id))
}

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
fn set_configstring(config_id: u32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }
    shinqlx_set_configstring(config_id, value);
    Ok(())
}

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
fn force_vote(pass: bool) -> bool {
    let current_level = CurrentLevel::default();
    let vote_time = current_level.get_vote_time();
    if vote_time.is_none() {
        return false;
    }

    let maxclients = unsafe { SV_MAXCLIENTS };
    for i in 0..maxclients {
        if let Ok(client) = Client::try_from(i) {
            if client.get_state() == CS_ACTIVE {
                client.set_vote(pass);
            }
        }
    }
    true
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
fn add_console_command(command: &str) {
    QuakeLiveEngine::default().add_command(command, cmd_py_command);
}

static mut CLIENT_COMMAND_HANDLER: Option<Py<PyAny>> = None;
static mut SERVER_COMMAND_HANDLER: Option<Py<PyAny>> = None;
static mut FRAME_HANDLER: Option<Py<PyAny>> = None;
static mut PLAYER_CONNECT_HANDLER: Option<Py<PyAny>> = None;
static mut PLAYER_LOADED_HANDLER: Option<Py<PyAny>> = None;
static mut PLAYER_DISCONNECT_HANDLER: Option<Py<PyAny>> = None;
pub(crate) static mut CUSTOM_COMMAND_HANDLER: Option<Py<PyAny>> = None;
static mut NEW_GAME_HANDLER: Option<Py<PyAny>> = None;
static mut SET_CONFIGSTRING_HANDLER: Option<Py<PyAny>> = None;
static mut RCON_HANDLER: Option<Py<PyAny>> = None;
static mut CONSOLE_PRINT_HANDLER: Option<Py<PyAny>> = None;
static mut PLAYER_SPAWN_HANDLER: Option<Py<PyAny>> = None;
static mut KAMIKAZE_USE_HANDLER: Option<Py<PyAny>> = None;
static mut KAMIKAZE_EXPLODE_HANDLER: Option<Py<PyAny>> = None;
static mut DAMAGE_HANDLER: Option<Py<PyAny>> = None;

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
fn register_handler(py: Python, event: &str, handler: Option<Py<PyAny>>) -> PyResult<()> {
    if handler
        .as_ref()
        .is_some_and(|handler_function| !handler_function.as_ref(py).is_callable())
    {
        return Err(PyTypeError::new_err("The handler must be callable."));
    }

    match event {
        "client_command" => unsafe { CLIENT_COMMAND_HANDLER = handler },
        "server_command" => unsafe { SERVER_COMMAND_HANDLER = handler },
        "frame" => unsafe { FRAME_HANDLER = handler },
        "player_connect" => unsafe { PLAYER_CONNECT_HANDLER = handler },
        "player_loaded" => unsafe { PLAYER_LOADED_HANDLER = handler },
        "player_disconnect" => unsafe { PLAYER_DISCONNECT_HANDLER = handler },
        "custom_command" => unsafe { CUSTOM_COMMAND_HANDLER = handler },
        "new_game" => unsafe { NEW_GAME_HANDLER = handler },
        "set_configstring" => unsafe { SET_CONFIGSTRING_HANDLER = handler },
        "rcon" => unsafe { RCON_HANDLER = handler },
        "console_print" => unsafe { CONSOLE_PRINT_HANDLER = handler },
        "player_spawn" => unsafe { PLAYER_SPAWN_HANDLER = handler },
        "kamikaze_use" => unsafe { KAMIKAZE_USE_HANDLER = handler },
        "kamikaze_explode" => unsafe { KAMIKAZE_EXPLODE_HANDLER = handler },
        "damage" => unsafe { DAMAGE_HANDLER = handler },
        _ => return Err(PyValueError::new_err("Unsupported event.")),
    };

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

        let mut results: [i32; 3] = [0; 3];
        for (item, result) in results.iter_mut().enumerate() {
            let extracted_value: PyResult<i32> = values.get_item(item).unwrap().extract();
            match extracted_value {
                Err(_) => return Err(PyValueError::new_err("Weapons values need to be boolean")),
                Ok(extracted_int) => *result = extracted_int,
            }
        }

        Ok(Vector3(results[0], results[1], results[2]))
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

        let mut results: [i32; 15] = [0; 15];
        for (item, result) in results.iter_mut().enumerate() {
            let extracted_value: PyResult<i32> = values.get_item(item).unwrap().extract();
            match extracted_value {
                Err(_) => return Err(PyValueError::new_err("Weapons values need to be boolean")),
                Ok(extracted_int) => *result = extracted_int,
            }
        }

        Ok(Weapons::from(results))
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

        let mut results: [i32; 6] = [0; 6];
        for (item, result) in results.iter_mut().enumerate() {
            let extracted_value: PyResult<i32> = values.get_item(item).unwrap().extract();
            match extracted_value {
                Err(_) => return Err(PyValueError::new_err("Weapons values need to be boolean")),
                Ok(extracted_int) => *result = extracted_int,
            }
        }

        Ok(Self::from(results))
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

        let mut results: [i32; 4] = [0; 4];
        for (item, result) in results.iter_mut().enumerate() {
            let extracted_value: PyResult<i32> = values.get_item(item).unwrap().extract();
            match extracted_value {
                Err(_) => return Err(PyValueError::new_err("Weapons values need to be boolean")),
                Ok(extracted_int) => *result = extracted_int,
            }
        }

        Ok(Self(results[0], results[1], results[2], results[3]))
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
fn player_state(client_id: i32) -> PyResult<Option<PlayerState>> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
fn player_stats(client_id: i32) -> PyResult<Option<PlayerStats>> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
fn set_position(client_id: i32, position: Vector3) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
            mutable_client.set_position((position.0 as f32, position.1 as f32, position.2 as f32));
            Ok(true)
        }
    }
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
fn set_velocity(client_id: i32, velocity: Vector3) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
            mutable_client.set_velocity((velocity.0 as f32, velocity.1 as f32, velocity.2 as f32));
            Ok(true)
        }
    }
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
fn noclip(client_id: i32, activate: bool) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's health.
#[pyfunction]
#[pyo3(name = "set_health")]
fn set_health(client_id: i32, health: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's armor.
#[pyfunction]
#[pyo3(name = "set_armor")]
fn set_armor(client_id: i32, armor: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
fn set_weapons(client_id: i32, weapons: Weapons) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
fn set_weapon(client_id: i32, weapon: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's ammo.
#[pyfunction]
#[pyo3(name = "set_ammo")]
fn set_ammo(client_id: i32, ammos: Weapons) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
fn set_powerups(client_id: i32, powerups: Powerups) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
fn set_holdable(client_id: i32, holdable: Holdable) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
            game_client.set_holdable(holdable as i32);
            Ok(true)
        }
    }
}

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
fn drop_holdable(client_id: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
            let mut game_client = game_entity.get_game_client().unwrap();
            game_client.remove_kamikaze_flag();
            if Holdable::from(game_client.get_holdable()) == Holdable::None {
                return Ok(false);
            }
            game_entity.drop_holdable();
            Ok(true)
        }
    }
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
fn set_flight(client_id: i32, flight: Flight) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
fn set_invulnerability(client_id: i32, time: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Makes player invulnerable for limited time.
#[pyfunction]
#[pyo3(name = "set_score")]
fn set_score(client_id: i32, score: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
fn callvote(vote: &str, vote_disp: &str, vote_time: Option<i32>) {
    let mut current_level = CurrentLevel::default();
    current_level.callvote(vote, vote_disp, vote_time);
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
fn allow_single_player(allow: bool) {
    let mut current_level = CurrentLevel::default();
    current_level.set_training_map(allow);
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
fn player_spawn(client_id: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
fn set_privileges(client_id: i32, privileges: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
fn destroy_kamikaze_timers() -> PyResult<bool> {
    for i in 0..MAX_GENTITIES {
        if let Ok(game_entity) = GameEntity::try_from(i) {
            if game_entity.in_use() {
                if game_entity.get_health() <= 0 {
                    if let Ok(game_client) = game_entity.get_game_client() {
                        let mut mut_game_client = game_client;
                        mut_game_client.remove_kamikaze_flag();
                    }
                }

                if game_entity.is_kamikaze_timer() {
                    let mut mut_entity = game_entity;
                    mut_entity.free_entity();
                }
            }
        }
    }
    Ok(true)
}

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
#[pyo3(signature = (item_id, x, y, z))]
fn spawn_item(item_id: i32, x: i32, y: i32, z: i32) -> PyResult<bool> {
    extern "C" {
        static bg_numItems: c_int;
    }

    let max_items: i32 = unsafe { bg_numItems };
    if !(1..max_items).contains(&item_id) {
        return Err(PyValueError::new_err(format!(
            "item_id needs to be a number from 1 to {}.",
            max_items - 1
        )));
    }

    GameEntity::spawn_item(item_id, (x, y, z));
    Ok(true)
}

/// Removes all dropped items.
#[pyfunction]
#[pyo3(name = "remove_dropped_items")]
fn remove_dropped_items() -> PyResult<bool> {
    for i in 0..MAX_GENTITIES {
        if let Ok(game_entity) = GameEntity::try_from(i) {
            if game_entity.in_use() && game_entity.has_flags() && game_entity.is_dropped_item() {
                let mut mut_entity = game_entity;
                mut_entity.free_entity();
            }
        }
    }
    Ok(true)
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "slay_with_mod")]
fn slay_with_mod(client_id: i32, mean_of_death: i32) -> PyResult<bool> {
    let maxclients = unsafe { SV_MAXCLIENTS };
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
}

fn determine_item_id(item: &PyAny) -> PyResult<i32> {
    extern "C" {
        static bg_numItems: c_int;
        static bg_itemlist: *const gitem_t;
    }

    if let Some(item_id) = py_extract_int_value(item) {
        if item_id < 0 || item_id >= unsafe { bg_numItems } {
            return Err(PyValueError::new_err(format!(
                "item_id needs to be between 0 and {}.",
                unsafe { bg_numItems - 1 }
            )));
        }
        return Ok(item_id);
    }

    if let Some(item_classname) = py_extract_str_value(item) {
        for i in 1..unsafe { bg_numItems } {
            if let Some(bg_item) = unsafe { bg_itemlist.offset(i as isize).as_ref() } {
                let game_item_classname =
                    unsafe { CStr::from_ptr(bg_item.classname).to_str().unwrap_or("") };
                if game_item_classname == item_classname {
                    return Ok(i);
                }
            }
        }
        return Err(PyValueError::new_err(format!(
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

    if let Some(item1_id) = py_extract_int_value(item1.as_ref(py)) {
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

    if let Some(item1_classname) = py_extract_str_value(item1.as_ref(py)) {
        let mut is_entity_found = false;
        for i in 0..MAX_GENTITIES {
            if let Ok(game_entity) = GameEntity::try_from(i as i32) {
                if game_entity.in_use()
                    && game_entity.is_game_item(ET_ITEM)
                    && game_entity.get_classname() == item1_classname
                {
                    is_entity_found = true;
                    let mut mut_game_entity = game_entity;
                    mut_game_entity.replace_item(item2_id);
                }
            }
        }
        return Ok(is_entity_found);
    }

    Err(PyValueError::new_err(
        "entity needs to be type of int or string.",
    ))
}

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
fn dev_print_items() {
    let mut formatted_items: Vec<String> = Vec::new();
    for i in 0..MAX_GENTITIES {
        match GameEntity::try_from(i as i32) {
            Err(_) => continue,
            Ok(game_entity) => {
                if !game_entity.in_use() {
                    continue;
                }
                if !game_entity.is_game_item(ET_ITEM) {
                    continue;
                }
                formatted_items.push(format!("{} {}", i, game_entity.get_classname()));
            }
        }
    }
    let mut str_length = 0;
    let printed_items: Vec<String> = formatted_items
        .iter()
        .take_while(|item| {
            str_length += item.len();
            str_length < 1024
        })
        .map(|item| item.to_string())
        .collect();
    let quake_live_engine = QuakeLiveEngine::default();
    if printed_items.is_empty() {
        quake_live_engine.send_server_command(None, "print \"No items found in the map\n\"");
        return;
    }
    quake_live_engine.send_server_command(
        None,
        format!("print \"{}\n\"", printed_items.join("\n")).as_str(),
    );

    let remaining_items: Vec<String> = formatted_items
        .iter()
        .skip(printed_items.len())
        .map(|item| item.to_string())
        .collect();

    if !remaining_items.is_empty() {
        quake_live_engine
            .send_server_command(None, "print \"Check server console for other items\n\"\n");
        for item in remaining_items {
            quake_live_engine.com_printf(item.as_str());
        }
    }
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "force_weapon_respawn_time")]
fn force_weapon_respawn_time(respawn_time: i32) -> PyResult<bool> {
    if respawn_time < 0 {
        return Err(PyValueError::new_err(
            "respawn time needs to be an integer 0 or greater",
        ));
    }

    for i in 0..MAX_GENTITIES {
        if let Ok(game_entity) = GameEntity::try_from(i as i32) {
            if game_entity.in_use() && game_entity.is_respawning_weapon() {
                let mut mut_entity = game_entity;
                mut_entity.set_respawn_time(respawn_time);
            }
        }
    }

    Ok(true)
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
    m.add("DAMAGE_RADIUS", DAMAGE_NO_ARMOR as i32)?;
    m.add("DAMAGE_RADIUS", DAMAGE_NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_RADIUS", DAMAGE_NO_PROTECTION as i32)?;
    m.add("DAMAGE_RADIUS", DAMAGE_NO_TEAM_PROTECTION as i32)?;

    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;
    m.add_class::<Vector3>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Flight>()?;

    Ok(())
}

pub(crate) static mut PYMINQLX_INITIALIZED: bool = false;

pub(crate) fn pyminqlx_is_initialized() -> bool {
    unsafe { PYMINQLX_INITIALIZED }
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
        let sys_module = py.import("sys")?;
        let path_attr = sys_module.getattr("path")?;
        let path_list: &PyList = path_attr.extract()?;
        path_list.append("minqlx.zip")?;
        path_list.append(".")?;
        let minqlx_module = py.import("minqlx")?;
        minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(_) => {
            #[cfg(debug_assertions)]
            println!("loader sequence returned an error. Did you modify the loader?");
            PYM_MAIN_SCRIPT_ERROR
        }
        Ok(_) => {
            unsafe { PYMINQLX_INITIALIZED = true };
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

    unsafe {
        CLIENT_COMMAND_HANDLER = None;
        SERVER_COMMAND_HANDLER = None;
        FRAME_HANDLER = None;
        PLAYER_CONNECT_HANDLER = None;
        PLAYER_LOADED_HANDLER = None;
        PLAYER_DISCONNECT_HANDLER = None;
        CUSTOM_COMMAND_HANDLER = None;
        NEW_GAME_HANDLER = None;
        SET_CONFIGSTRING_HANDLER = None;
        RCON_HANDLER = None;
        CONSOLE_PRINT_HANDLER = None;
        PLAYER_SPAWN_HANDLER = None;
        KAMIKAZE_USE_HANDLER = None;
        KAMIKAZE_EXPLODE_HANDLER = None;
        DAMAGE_HANDLER = None;
    }

    match Python::with_gil(|py| {
        let importlib_module = py.import("importlib")?;
        let minqlx_module = py.import("minqlx")?;
        let new_minqlx_module = importlib_module.call_method1("reload", (minqlx_module,))?;
        new_minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(_) => {
            unsafe {
                PYMINQLX_INITIALIZED = false;
            }
            PYM_MAIN_SCRIPT_ERROR
        }
        Ok(()) => {
            unsafe {
                PYMINQLX_INITIALIZED = true;
            }
            PYM_SUCCESS
        }
    }
}
