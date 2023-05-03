use crate::commands::cmd_py_command;
use crate::hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
use crate::quake_common::clientState_t::{CS_ACTIVE, CS_FREE, CS_ZOMBIE};
use crate::quake_common::team_t::TEAM_SPECTATOR;
use crate::quake_common::{
    AddCommand, Client, ConsoleCommand, CurrentLevel, FindCVar, GameClient, GameEntity,
    GetConfigstring, QuakeLiveEngine, SetCVar, SetCVarForced, SetCVarLimit, MAX_CONFIGSTRINGS,
};
use crate::SV_MAXCLIENTS;
use lazy_static::lazy_static;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::c_int;
use std::string::ToString;
use std::sync::Mutex;

/// Information about a player, such as Steam ID, name, client ID, and whatnot.
#[pyclass]
#[pyo3(name = "PlayerInfo")]
#[allow(unused)]
struct PlayerInfo {
    /// The player's client ID.
    client_id: i32,
    /// The player's name.
    name: Cow<'static, str>,
    /// The player's connection state.
    connection_state: i32,
    /// The player's userinfo.
    userinfo: Cow<'static, str>,
    /// The player's 64-bit representation of the Steam ID.
    steam_id: u64,
    /// The player's team.
    team: i32,
    /// The player's privileges.
    privileges: i32,
}

fn make_player_tuple(client_id: i32) -> Option<PlayerInfo> {
    let client_result = Client::try_from(client_id);
    match client_result {
        Err(_) => Some(PlayerInfo {
            client_id,
            name: Default::default(),
            connection_state: 0,
            userinfo: Default::default(),
            steam_id: 0,
            team: TEAM_SPECTATOR as i32,
            privileges: -1,
        }),
        Ok(client) => Some(PlayerInfo {
            client_id,
            name: client.get_player_name(),
            connection_state: client.get_state(),
            userinfo: client.get_user_info(),
            steam_id: client.get_steam_id(),
            team: client.get_team(),
            privileges: client.get_privileges(),
        }),
    }
}

extern "C" {
    static allow_free_client: c_int;
}

/// Returns a dictionary with information about a player by ID.
#[pyfunction]
#[pyo3(name = "player_info")]
fn get_player_info(client_id: i32) -> PyResult<Option<PlayerInfo>> {
    if !(0..*SV_MAXCLIENTS.lock().unwrap()).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            *SV_MAXCLIENTS.lock().unwrap() - 1
        )));
    }
    if let Ok(client) = Client::try_from(client_id) {
        if unsafe { allow_free_client } != client_id && client.get_state() == CS_FREE as i32 {
            #[cfg(debug_assertions)]
            println!(
                "WARNING: get_player_info called for CS_FREE client {}.",
                client_id
            );
            return Ok(None);
        }
    }

    Ok(make_player_tuple(client_id))
}

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction]
#[pyo3(name = "players_info")]
fn get_players_info() -> PyResult<Vec<Option<PlayerInfo>>> {
    let mut result = Vec::new();
    for client_id in 0..*SV_MAXCLIENTS.lock().unwrap() {
        match Client::try_from(client_id) {
            Err(_) => result.push(None),
            Ok(client) => {
                if client.get_state() == CS_FREE as i32 {
                    result.push(None)
                } else {
                    result.push(make_player_tuple(client_id))
                }
            }
        }
    }
    Ok(result)
}

/// Returns a string with a player's userinfo.
#[pyfunction]
#[pyo3(name = "get_userinfo")]
fn get_userinfo(client_id: i32) -> PyResult<Option<Cow<'static, str>>> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    match Client::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(client) => {
            if unsafe { allow_free_client } != client_id && client.get_state() == CS_FREE as i32 {
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
#[pyo3(signature = (optional_client_id, cmd))]
fn send_server_command(optional_client_id: Option<i32>, cmd: &str) -> PyResult<Option<bool>> {
    match optional_client_id {
        None => {
            shinqlx_send_server_command(None, cmd);
            Ok(Some(true))
        }
        Some(client_id) => {
            let maxclients = *SV_MAXCLIENTS.lock().unwrap();
            if !(0..maxclients).contains(&client_id) {
                return Err(PyValueError::new_err(format!(
                    "client_id needs to be a number from 0 to {}, or None.",
                    maxclients - 1
                )));
            }
            match Client::try_from(client_id) {
                Err(_) => Ok(Some(false)),
                Ok(client) => {
                    if client.get_state() != CS_ACTIVE as i32 {
                        Ok(Some(false))
                    } else {
                        shinqlx_send_server_command(Some(client), cmd);
                        Ok(Some(true))
                    }
                }
            }
        }
    }
}

/// Tells the server to process a command from a specific client.
#[pyfunction]
#[pyo3(name = "client_command")]
#[pyo3(signature = (optional_client_id, cmd))]
fn client_command(optional_client_id: Option<i32>, cmd: &str) -> PyResult<Option<bool>> {
    match optional_client_id {
        None => {
            shinqlx_execute_client_command(None, cmd, true);
            Ok(Some(true))
        }
        Some(client_id) => {
            let maxclients = *SV_MAXCLIENTS.lock().unwrap();
            if !(0..maxclients).contains(&client_id) {
                return Err(PyValueError::new_err(format!(
                    "client_id needs to be a number from 0 to {}, or None.",
                    maxclients - 1
                )));
            }
            match Client::try_from(client_id) {
                Err(_) => Ok(Some(false)),
                Ok(client) => {
                    if [CS_FREE as i32, CS_ZOMBIE as i32].contains(&client.get_state()) {
                        Ok(Some(false))
                    } else {
                        shinqlx_execute_client_command(Some(client), cmd, true);
                        Ok(Some(true))
                    }
                }
            }
        }
    }
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
#[pyo3(signature = (cmd))]
fn console_command(cmd: &str) {
    QuakeLiveEngine::execute_console_command(cmd);
}

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
#[pyo3(signature = (cvar))]
fn get_cvar(cvar: &str) -> PyResult<Option<String>> {
    match QuakeLiveEngine::find_cvar(cvar) {
        None => Ok(None),
        Some(cvar_result) => Ok(Some(cvar_result.get_string().to_string())),
    }
}

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
fn set_cvar(cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
    match QuakeLiveEngine::find_cvar(cvar) {
        None => {
            QuakeLiveEngine::set_cvar(cvar, value, flags);
            Ok(true)
        }
        Some(_) => {
            if flags.is_none() {
                QuakeLiveEngine::set_cvar_forced(cvar, value, true);
            } else {
                QuakeLiveEngine::set_cvar_forced(cvar, value, false);
            }
            Ok(false)
        }
    }
}

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
fn set_cvar_limit(cvar: &str, value: &str, min: &str, max: &str, flags: Option<i32>) {
    QuakeLiveEngine::set_cvar_limit(cvar, value, min, max, flags);
}

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
fn kick(client_id: i32, reason: Option<&str>) -> PyResult<()> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    match Client::try_from(client_id) {
        Err(_) => Err(PyValueError::new_err(
            "client_id must be None or the ID of an active player.",
        )),
        Ok(client) => {
            if client.get_state() != CS_ACTIVE as i32 {
                return Err(PyValueError::new_err(
                    "client_id must be None or the ID of an active player.",
                ));
            }
            shinqlx_drop_client(&client, reason.unwrap_or("was kicked."));
            Ok(())
        }
    }
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
#[pyo3(signature = (text))]
fn console_print(text: &str) {
    shinqlx_com_printf(text);
}

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
#[pyo3(signature = (config_id))]
fn get_configstring(config_id: i32) -> PyResult<Cow<'static, str>> {
    if !(0..MAX_CONFIGSTRINGS as i32).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }
    Ok(Cow::from(QuakeLiveEngine::get_configstring(config_id)))
}

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
#[pyo3(signature = (config_id, value))]
fn set_configstring(config_id: i32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS as i32).contains(&config_id) {
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
#[pyo3(signature = (pass))]
fn force_vote(pass: bool) -> bool {
    let mut current_level = CurrentLevel::default();
    let vote_time = current_level.get_vote_time();
    if vote_time.is_none() {
        return false;
    }

    if !pass {
        current_level.set_vote_time(-1);
    } else {
        for i in 0..*SV_MAXCLIENTS.lock().unwrap() {
            if let Ok(client) = Client::try_from(i) {
                if client.get_state() == CS_ACTIVE as i32 {
                    client.set_vote(true);
                }
            }
        }
    }

    true
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
#[pyo3(signature = (command))]
fn add_console_command(command: &str) {
    QuakeLiveEngine::add_command(command, cmd_py_command);
}

lazy_static! {
    pub(crate) static ref HANDLERS: Mutex<HashMap<String, Py<PyAny>>> = Mutex::new(HashMap::new());
    pub(crate) static ref SUPPORTED_HANDLERS: Vec<&'static str> = Vec::from([
        "client_command",
        "server_command",
        "frame",
        "player_connect",
        "player_loaded",
        "player_disconnect",
        "custom_command",
        "new_game",
        "set_configstring",
        "rcon",
        "console_print",
        "player_spawn",
        "kamikaze_use",
        "kamikaze_explode",
    ]);
}

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
fn register_handler(py: Python<'_>, event: &str, handler: Option<Py<PyAny>>) -> PyResult<()> {
    if let Some(handler_function) = &handler {
        if !handler_function.as_ref(py).is_callable() {
            return Err(PyTypeError::new_err("The handler must be callable."));
        }
    }

    if !SUPPORTED_HANDLERS.contains(&event) {
        return Err(PyValueError::new_err("Unsupported event."));
    }

    match handler {
        None => HANDLERS.lock().unwrap().remove(event),
        Some(python_handler) => HANDLERS
            .lock()
            .unwrap()
            .insert(event.to_string(), python_handler),
    };

    Ok(())
}

/// A three-dimensional vector.
#[pyclass]
#[pyo3(name = "Vector3")]
#[derive(PartialEq, Debug, Clone, Copy)]
struct Vector3 {
    x: i32,
    y: i32,
    z: i32,
}

impl From<(f32, f32, f32)> for Vector3 {
    fn from(value: (f32, f32, f32)) -> Self {
        Self {
            x: value.0 as i32,
            y: value.1 as i32,
            z: value.2 as i32,
        }
    }
}

/// A struct sequence containing all the weapons in the game.
#[pyclass]
#[pyo3(name = "Weapons")]
#[derive(PartialEq, Debug, Clone, Copy)]
struct Weapons {
    g: bool,
    mg: bool,
    sg: bool,
    gl: bool,
    rl: bool,
    lg: bool,
    rg: bool,
    pg: bool,
    bfg: bool,
    gh: bool,
    ng: bool,
    pl: bool,
    cg: bool,
    hmg: bool,
    hands: bool,
}

impl From<[bool; 15]> for Weapons {
    fn from(value: [bool; 15]) -> Self {
        Self {
            g: value[0],
            mg: value[1],
            sg: value[2],
            gl: value[3],
            rl: value[4],
            lg: value[5],
            rg: value[6],
            pg: value[7],
            bfg: value[8],
            gh: value[9],
            ng: value[10],
            pl: value[11],
            cg: value[12],
            hmg: value[13],
            hands: value[14],
        }
    }
}

impl From<Weapons> for [bool; 15] {
    fn from(value: Weapons) -> Self {
        [
            value.g,
            value.mg,
            value.sg,
            value.gl,
            value.rl,
            value.lg,
            value.rg,
            value.pg,
            value.bfg,
            value.gh,
            value.ng,
            value.pl,
            value.cg,
            value.hmg,
            value.hands,
        ]
    }
}

/// A struct sequence containing all the different ammo types for the weapons in the game.
#[pyclass]
#[pyo3(name = "Ammo")]
#[derive(PartialEq, Debug, Clone, Copy)]
struct Ammo {
    g: i32,
    mg: i32,
    sg: i32,
    gl: i32,
    rl: i32,
    lg: i32,
    rg: i32,
    pg: i32,
    bfg: i32,
    gh: i32,
    ng: i32,
    pl: i32,
    cg: i32,
    hmg: i32,
    hands: i32,
}

impl From<[i32; 15]> for Ammo {
    fn from(value: [i32; 15]) -> Self {
        Self {
            g: value[0],
            mg: value[1],
            sg: value[2],
            gl: value[3],
            rl: value[4],
            lg: value[5],
            rg: value[6],
            pg: value[7],
            bfg: value[8],
            gh: value[9],
            ng: value[10],
            pl: value[11],
            cg: value[12],
            hmg: value[13],
            hands: value[14],
        }
    }
}

impl From<Ammo> for [i32; 15] {
    fn from(value: Ammo) -> Self {
        [
            value.g,
            value.mg,
            value.sg,
            value.gl,
            value.rl,
            value.lg,
            value.rg,
            value.pg,
            value.bfg,
            value.gh,
            value.ng,
            value.pl,
            value.cg,
            value.hmg,
            value.hands,
        ]
    }
}

/// A struct sequence containing all the powerups in the game.
#[pyclass]
#[pyo3(name = "Powerups")]
#[derive(PartialEq, Debug, Clone, Copy)]
struct Powerups {
    quad: i32,
    battlesuit: i32,
    haste: i32,
    invisibility: i32,
    regeneration: i32,
    invulnerability: i32,
}

impl From<[i32; 6]> for Powerups {
    fn from(value: [i32; 6]) -> Self {
        Self {
            quad: value[0],
            battlesuit: value[1],
            haste: value[2],
            invisibility: value[3],
            regeneration: value[4],
            invulnerability: value[5],
        }
    }
}

impl From<Powerups> for [i32; 6] {
    fn from(value: Powerups) -> Self {
        [
            value.quad,
            value.battlesuit,
            value.haste,
            value.invisibility,
            value.regeneration,
            value.invulnerability,
        ]
    }
}

#[pyclass]
#[pyo3(name = "Holdable")]
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
#[pyo3(name = "Flight")]
#[derive(PartialEq, Debug, Clone, Copy)]
struct Flight {
    fuel: i32,
    max_fuel: i32,
    thrust: i32,
    refuel: i32,
}

impl From<Flight> for (i32, i32, i32, i32) {
    fn from(flight: Flight) -> Self {
        (flight.fuel, flight.max_fuel, flight.thrust, flight.refuel)
    }
}

/// Information about a player's state in the game.
#[pyclass]
#[pyo3(name = "PlayerState")]
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
    ammo: Ammo,
    ///The player's powerups.
    powerups: Powerups,
    /// The player's holdable item.
    holdable: Option<Cow<'static, str>>,
    /// A struct sequence with flight parameters.
    flight: Flight,
    /// Whether the player is frozen(freezetag).
    is_frozen: bool,
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
            weapon: game_client.get_weapon(),
            weapons: Weapons::from(game_client.get_weapons()),
            ammo: Ammo::from(game_client.get_ammo()),
            powerups: Powerups::from(game_client.get_powerups()),
            holdable: holdable_from(game_client.get_holdable().into()),
            flight: Flight {
                fuel: game_client.get_current_flight_fuel(),
                max_fuel: game_client.get_max_flight_fuel(),
                thrust: game_client.get_flight_thrust(),
                refuel: game_client.get_flight_refuel(),
            },
            is_frozen: game_client.is_frozen(),
        }
    }
}

fn holdable_from(holdable: Holdable) -> Option<Cow<'static, str>> {
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
#[pyo3(signature = (client_id))]
fn player_state(client_id: i32) -> PyResult<Option<PlayerState>> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    match GameEntity::try_from(client_id) {
        Err(_) => Ok(None),
        Ok(game_entity) => Ok(Some(PlayerState::from(game_entity))),
    }
}

/// A player's score and some basic stats.
#[pyclass]
#[pyo3(name = "PlayerStats")]
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
#[pyo3(signature = (client_id))]
fn player_stats(client_id: i32) -> PyResult<Option<PlayerStats>> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, position))]
fn set_position(client_id: i32, position: Vector3) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
            mutable_client.set_position((position.x as f32, position.y as f32, position.z as f32));
            Ok(true)
        }
    }
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
#[pyo3(signature = (client_id, velocity))]
fn set_velocity(client_id: i32, velocity: Vector3) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
            mutable_client.set_velocity((velocity.x as f32, velocity.y as f32, velocity.z as f32));
            Ok(true)
        }
    }
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
#[pyo3(signature = (client_id, activate))]
fn noclip(client_id: i32, activate: bool) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, health))]
fn set_health(client_id: i32, health: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, armor))]
fn set_armor(client_id: i32, armor: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, weapons))]
fn set_weapons(client_id: i32, weapons: Weapons) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, weapon))]
fn set_weapon(client_id: i32, weapon: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, ammos))]
fn set_ammo(client_id: i32, ammos: Ammo) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, powerups))]
fn set_powerups(client_id: i32, powerups: Powerups) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, holdable))]
fn set_holdable(client_id: i32, holdable: Holdable) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id))]
fn drop_holdable(client_id: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, flight))]
fn set_flight(client_id: i32, flight: Flight) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
            game_client.set_flight(flight.into());
            Ok(true)
        }
    }
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
#[pyo3(signature = (client_id, time))]
fn set_invulnerability(client_id: i32, time: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (client_id, score))]
fn set_score(client_id: i32, score: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
#[pyo3(signature = (vote, vote_disp))]
fn callvote(vote: &str, vote_disp: &str) {
    let mut current_level = CurrentLevel::default();
    current_level.callvote(vote, vote_disp);
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
#[pyo3(signature = (allow))]
fn allow_single_player(allow: bool) {
    let mut current_level = CurrentLevel::default();
    current_level.set_training_map(allow);
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
#[pyo3(signature = (client_id))]
fn player_spawn(client_id: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
            game_client.spawn();
            shinqlx_client_spawn(game_entity);
            Ok(true)
        }
    }
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
#[pyo3(signature = (client_id, privileges))]
fn set_privileges(client_id: i32, privileges: i32) -> PyResult<bool> {
    let maxclients = *SV_MAXCLIENTS.lock().unwrap();
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
            game_client.set_privileges(privileges);
            Ok(true)
        }
    }
}

#[pymodule]
#[pyo3(name = "_minqlx")]
fn pyminqlx_init_module(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
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

    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;
    m.add_class::<Vector3>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Flight>()?;
    Ok(())
}
