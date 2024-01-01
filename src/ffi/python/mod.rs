mod channels;
pub(crate) mod dispatchers;
pub(crate) mod embed;
mod flight;
mod game;
mod holdable;
mod player;
mod player_info;
mod player_state;
mod player_stats;
mod powerups;
mod vector3;
mod weapons;

use embed::*;
pub(crate) use flight::Flight;
use game::{Game, NonexistentGameError};
pub(crate) use holdable::Holdable;
use player::{AbstractDummyPlayer, NonexistentPlayerError, Player, RconDummyPlayer};
pub(crate) use player_info::PlayerInfo;
pub(crate) use player_state::PlayerState;
pub(crate) use player_stats::PlayerStats;
pub(crate) use powerups::Powerups;
pub(crate) use vector3::Vector3;
pub(crate) use weapons::Weapons;

use crate::prelude::*;

use crate::ffi::python::channels::{
    AbstractChannel, ChatChannel, ClientCommandChannel, ConsoleChannel, TeamChatChannel,
    TellChannel, MAX_MSG_LENGTH,
};
use crate::_INIT_TIME;
use alloc::vec::IntoIter;
use core::str::FromStr;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use itertools::Itertools;
use log::*;
use once_cell::sync::Lazy;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDelta, PyDict, PyFunction, PyTuple};
use pyo3::{append_to_inittab, create_exception, prepare_freethreaded_python};
use regex::Regex;
use swap_arc::SwapArcOption;

pub(crate) static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) static CLIENT_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static SERVER_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static FRAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static PLAYER_CONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static PLAYER_LOADED_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static PLAYER_DISCONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static CUSTOM_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static NEW_GAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static SET_CONFIGSTRING_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static RCON_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static CONSOLE_PRINT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static PLAYER_SPAWN_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static KAMIKAZE_USE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static KAMIKAZE_EXPLODE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static DAMAGE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));

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

create_exception!(pyshinqlx_module, PluginLoadError, PyException);
create_exception!(pyshinqlx_module, PluginUnloadError, PyException);

pub(crate) fn clean_text<T>(text: &T) -> String
where
    T: AsRef<str>,
{
    let re = Regex::new(r#"\^[0-7]"#).unwrap();
    re.replace_all(text.as_ref(), "").into()
}

pub(crate) fn parse_variables(varstr: String) -> ParsedVariables {
    varstr
        .parse::<ParsedVariables>()
        .unwrap_or(ParsedVariables { items: vec![] })
}

pub(crate) struct ParsedVariables {
    items: Vec<(String, String)>,
}

impl FromStr for ParsedVariables {
    type Err = &'static str;

    fn from_str(varstr: &str) -> Result<Self, Self::Err> {
        let Some(varstr_vec): Option<Vec<String>> = varstr
            .strip_prefix('\\')
            .map(|value| value.split('\\').map(|value| value.into()).collect())
        else {
            return Ok(Self { items: vec![] });
        };

        if varstr_vec.len() % 2 == 1 {
            warn!(target: "shinqlx", "Uneven number of keys and values: {}", varstr);
        }
        Ok(Self {
            items: varstr_vec.into_iter().tuples().collect(),
        })
    }
}

impl From<ParsedVariables> for String {
    fn from(value: ParsedVariables) -> Self {
        value
            .items
            .iter()
            .map(|(key, value)| format!("\\{key}\\{value}"))
            .join("")
    }
}

impl IntoPy<PyObject> for ParsedVariables {
    fn into_py(self, py: Python<'_>) -> PyObject {
        self.items.into_py_dict(py).into()
    }
}

impl IntoIterator for ParsedVariables {
    type Item = (String, String);
    type IntoIter = IntoIter<(String, String)>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl ParsedVariables {
    pub fn get<T>(&self, item: T) -> Option<String>
    where
        T: AsRef<str>,
    {
        self.items
            .iter()
            .filter(|(key, _value)| key == item.as_ref())
            .map(|(_key, value)| value)
            .next()
            .cloned()
    }

    pub fn set(&mut self, item: String, value: String) {
        let mut new_items: Vec<(String, String)> = self
            .items
            .clone()
            .into_iter()
            .filter(|(key, _value)| *key != item)
            .collect();
        new_items.push((item, value));
        self.items = new_items;
    }
}

#[pyfunction]
#[pyo3(pass_module)]
fn set_map_subtitles(module: &PyModule) -> PyResult<()> {
    let map_title = pyshinqlx_get_configstring(module.py(), 3)?;
    module.setattr("_map_title", map_title)?;

    let mut map_subtitle1 = pyshinqlx_get_configstring(module.py(), 678)?;
    module.setattr("_map_subtitle1", map_subtitle1.clone())?;

    let mut map_subtitle2 = pyshinqlx_get_configstring(module.py(), 679)?;
    module.setattr("_map_subtitle2", map_subtitle2.clone())?;

    if !map_subtitle1.is_empty() {
        map_subtitle1.push_str(" - ");
    }

    map_subtitle1.push_str("Running shinqlx ^6");
    map_subtitle1.push_str(env!("SHINQLX_VERSION"));
    map_subtitle1.push_str("^7 with plugins ^6");
    let plugins_version = module
        .getattr("__plugins_version__")
        .map(|value| value.extract::<String>().unwrap_or("NOT_SET".into()))
        .unwrap_or("NOT_SET".into());
    map_subtitle1.push_str(&plugins_version);
    map_subtitle1.push_str("^7.");

    pyshinqlx_set_configstring(module.py(), 678, &map_subtitle1)?;

    if !map_subtitle2.is_empty() {
        map_subtitle2.push_str(" - ");
    }
    map_subtitle2.push_str("Check ^6https://github.com/mgaertne/shinqlx^7 for more details.");
    pyshinqlx_set_configstring(module.py(), 679, &map_subtitle2)?;

    Ok(())
}

/// Parses strings of key-value pairs delimited by "\\" and puts
/// them into a dictionary.
#[pyfunction]
#[pyo3(name = "parse_variables")]
#[pyo3(signature = (varstr, ordered=false))]
fn pyshinqlx_parse_variables(
    py: Python<'_>,
    varstr: String,
    #[allow(unused_variables)] ordered: bool,
) -> &PyDict {
    parse_variables(varstr).into_py_dict(py)
}

#[pyfunction]
fn next_frame(py: Python<'_>, func: Py<PyFunction>) -> PyResult<PyObject> {
    let next_frame_func: Py<PyAny> = PyModule::from_code(
        py,
        r#"
from functools import wraps

import shinqlx


def next_frame(func):
    @wraps(func)
    def f(*args, **kwargs):
        shinqlx.next_frame_tasks.put_nowait((func, args, kwargs))

    return f
        "#,
        "",
        "",
    )?
    .getattr("next_frame")?
    .into();

    next_frame_func.call1(py, (func.into_py(py),))
}

/// Delay a function call a certain amount of time.
///
///     .. note::
///         It cannot guarantee you that it will be called right as the timer
///         expires, but unless some plugin is for some reason blocking, then
///         you can expect it to be called practically as soon as it expires.
#[pyfunction]
fn delay(py: Python<'_>, time: f32) -> PyResult<PyObject> {
    let delay_func: Py<PyAny> = PyModule::from_code(
        py,
        r#"
from functools import wraps

import shinqlx


def delay(time):
    def wrap(func):
        @wraps(func)
        def f(*args, **kwargs):
            shinqlx.frame_tasks.enter(time, 1, func, args, kwargs)

        return f

    return wrap
    "#,
        "",
        "",
    )?
    .getattr("delay")?
    .into();

    delay_func.call1(py, (time.into_py(py),))
}

/// Starts a thread with the function passed as its target. If a function decorated
/// with this is called within a function also decorated, it will **not** create a second
/// thread unless told to do so with the *force* keyword.
#[pyfunction]
#[pyo3(signature = (func, force=false))]
fn thread(py: Python<'_>, func: Py<PyFunction>, force: bool) -> PyResult<PyObject> {
    let thread_func: Py<PyAny> = PyModule::from_code(
        py,
        r#"
import threading
from functools import wraps

import shinqlx


def thread(func, force=False):
    @wraps(func)
    def f(*args, **kwargs):
        if not force and threading.current_thread().name.endswith(shinqlx._thread_name):
            func(*args, **kwargs)
        else:
            name = f"{func.__name__}-{str(shinqlx._thread_count)}-{shinqlx._thread_name}"
            t = threading.Thread(
                target=func, name=name, args=args, kwargs=kwargs, daemon=True
            )
            t.start()
            shinqlx._thread_count += 1

            return t

    return f
        "#,
        "",
        "",
    )?
    .getattr("thread")?
    .into();

    thread_func.call1(py, (func.into_py(py), force.into_py(py)))
}

/// Returns a :class:`datetime.timedelta` instance of the time since initialized.
#[pyfunction]
fn uptime(py: Python<'_>) -> PyResult<&PyDelta> {
    let elapsed = _INIT_TIME.elapsed();
    let elapsed_days: i32 = (elapsed.as_secs() / (24 * 60 * 60))
        .try_into()
        .unwrap_or_default();
    let elapsed_seconds: i32 = (elapsed.as_secs() % (24 * 60 * 60))
        .try_into()
        .unwrap_or_default();
    let elapsed_microseconds: i32 = elapsed.subsec_micros().try_into().unwrap_or_default();
    PyDelta::new(
        py,
        elapsed_days,
        elapsed_seconds,
        elapsed_microseconds,
        false,
    )
}

/// Returns the SteamID64 of the owner. This is set in the config.
#[pyfunction]
fn owner(py: Python<'_>) -> PyResult<Option<u64>> {
    let Ok(Some(owner_cvar)) = pyshinqlx_get_cvar(py, "qlx_owner") else {
        error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
        return Ok(None);
    };

    let Ok(steam_id) = owner_cvar.parse::<i64>() else {
        error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
        return Ok(None);
    };

    if steam_id < 0 {
        error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
        return Ok(None);
    }

    Ok(Some(steam_id.try_into()?))
}

static DEFAULT_PLUGINS: [&str; 10] = [
    "plugin_manager",
    "essentials",
    "motd",
    "permission",
    "ban",
    "silence",
    "clan",
    "names",
    "log",
    "workshop",
];

#[pyfunction]
fn initialize_cvars(py: Python<'_>) -> PyResult<()> {
    pyshinqlx_set_cvar_once(py, "qlx_owner", "-1".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_plugins", DEFAULT_PLUGINS.join(", ").into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_pluginsPath", "shinqlx-plugins".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_database", "Redis".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_commandPrefix", "!".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_logs", "2".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_logsSize", "3000000".into_py(py), 0)?;

    pyshinqlx_set_cvar_once(py, "qlx_redisAddress", "127.0.0.1".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisDatabase", "0".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisUnixSocket", "0".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisPassword", "".into_py(py), 0)?;
    Ok(())
}

#[pymodule]
#[pyo3(name = "shinqlx")]
fn pyshinqlx_root_module(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    Ok(())
}

#[pymodule]
#[pyo3(name = "_shinqlx")]
fn pyshinqlx_module(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pyshinqlx_player_info, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_players_info, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_userinfo, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_send_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_client_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_limit, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_kick, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_console_print, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_force_vote, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_add_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_register_handler, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_state, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_stats, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_position, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_velocity, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_noclip, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_health, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_armor, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_weapons, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_weapon, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_ammo, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_powerups, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_drop_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_flight, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_invulnerability, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_score, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_callvote, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_allow_single_player, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_privileges, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_destroy_kamikaze_timers, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_spawn_item, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_remove_dropped_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_slay_with_mod, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_replace_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_dev_print_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_force_weapon_respawn_time, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_entity_targets, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_once, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_limit_once, m)?)?;

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

    // Game types
    m.add(
        "GAMETYPES",
        [
            (0, "Free for All"),
            (1, "Duel"),
            (2, "Race"),
            (3, "Team Deathmatch"),
            (4, "Clan Arena"),
            (5, "Capture the Flag"),
            (6, "One Flag"),
            (8, "Harvester"),
            (9, "Freeze Tag"),
            (10, "Domination"),
            (11, "Attack and Defend"),
            (12, "Red Rover"),
        ]
        .into_py_dict(py),
    )?;
    m.add(
        "GAMETYPES_SHORT",
        [
            (0, "ffa"),
            (1, "duel"),
            (2, "race"),
            (3, "tdm"),
            (4, "ca"),
            (5, "ctf"),
            (6, "1f"),
            (8, "har"),
            (9, "ft"),
            (10, "dom"),
            (11, "ad"),
            (12, "rr"),
        ]
        .into_py_dict(py),
    )?;

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
    m.add(
        "CONNECTION_STATES",
        [
            (clientState_t::CS_FREE as i32, "free"),
            (clientState_t::CS_ZOMBIE as i32, "zombie"),
            (clientState_t::CS_CONNECTED as i32, "connected"),
            (clientState_t::CS_PRIMED as i32, "primed"),
            (clientState_t::CS_ACTIVE as i32, "active"),
        ]
        .into_py_dict(py),
    )?;

    // Teams.
    m.add("TEAM_FREE", team_t::TEAM_FREE as i32)?;
    m.add("TEAM_RED", team_t::TEAM_RED as i32)?;
    m.add("TEAM_BLUE", team_t::TEAM_BLUE as i32)?;
    m.add("TEAM_SPECTATOR", team_t::TEAM_SPECTATOR as i32)?;
    m.add(
        "TEAMS",
        [
            (team_t::TEAM_FREE as i32, "free"),
            (team_t::TEAM_RED as i32, "red"),
            (team_t::TEAM_BLUE as i32, "blue"),
            (team_t::TEAM_SPECTATOR as i32, "spectator"),
        ]
        .into_py_dict(py),
    )?;

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

    // Weapons
    m.add(
        "WEAPONS",
        [
            (weapon_t::WP_GAUNTLET as i32, "g"),
            (weapon_t::WP_MACHINEGUN as i32, "mg"),
            (weapon_t::WP_SHOTGUN as i32, "sg"),
            (weapon_t::WP_GRENADE_LAUNCHER as i32, "gl"),
            (weapon_t::WP_ROCKET_LAUNCHER as i32, "rl"),
            (weapon_t::WP_LIGHTNING as i32, "lg"),
            (weapon_t::WP_RAILGUN as i32, "rg"),
            (weapon_t::WP_PLASMAGUN as i32, "pg"),
            (weapon_t::WP_BFG as i32, "bfg"),
            (weapon_t::WP_GRAPPLING_HOOK as i32, "gh"),
            (weapon_t::WP_NAILGUN as i32, "ng"),
            (weapon_t::WP_PROX_LAUNCHER as i32, "pl"),
            (weapon_t::WP_CHAINGUN as i32, "cg"),
            (weapon_t::WP_HMG as i32, "hmg"),
            (weapon_t::WP_HANDS as i32, "hands"),
        ]
        .into_py_dict(py),
    )?;

    m.add("DAMAGE_RADIUS", DAMAGE_RADIUS as i32)?;
    m.add("DAMAGE_NO_ARMOR", DAMAGE_NO_ARMOR as i32)?;
    m.add("DAMAGE_NO_KNOCKBACK", DAMAGE_NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_NO_PROTECTION", DAMAGE_NO_PROTECTION as i32)?;
    m.add(
        "DAMAGE_NO_TEAM_PROTECTION",
        DAMAGE_NO_TEAM_PROTECTION as i32,
    )?;

    m.add("DEFAULT_PLUGINS", PyTuple::new(py, DEFAULT_PLUGINS))?;

    m.add("_map_title", "")?;
    m.add("_map_subtitle1", "")?;
    m.add("_map_subtitle2", "")?;
    m.add_function(wrap_pyfunction!(set_map_subtitles, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_parse_variables, m)?)?;

    m.add_function(wrap_pyfunction!(next_frame, m)?)?;
    m.add_function(wrap_pyfunction!(delay, m)?)?;

    m.add("_thread_count", 0)?;
    m.add("_thread_name", "shinqlxthread")?;
    m.add_function(wrap_pyfunction!(thread, m)?)?;
    m.add_function(wrap_pyfunction!(initialize_cvars, m)?)?;

    m.add_function(wrap_pyfunction!(uptime, m)?)?;
    m.add_function(wrap_pyfunction!(owner, m)?)?;

    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;
    m.add_class::<Vector3>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Flight>()?;
    m.add_class::<Game>()?;
    m.add(
        "NonexistentGameError",
        py.get_type::<NonexistentGameError>(),
    )?;
    m.add_class::<Player>()?;
    m.add(
        "NonexistentPlayerError",
        py.get_type::<NonexistentPlayerError>(),
    )?;
    m.add_class::<AbstractDummyPlayer>()?;
    m.add_class::<RconDummyPlayer>()?;
    m.add("MAX_MSG_LENGTH", MAX_MSG_LENGTH)?;
    m.add_class::<AbstractChannel>()?;
    m.add_class::<ConsoleChannel>()?;
    m.add_class::<ChatChannel>()?;
    m.add_class::<TellChannel>()?;
    m.add_class::<ClientCommandChannel>()?;
    m.add_class::<TeamChatChannel>()?;
    m.add(
        "CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("all".into(), "chat".into(), "print \"{}\n\"\n".into()),
        )?
        .to_object(py),
    )?;
    m.add(
        "RED_TEAM_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new(
                "red".into(),
                "red_team_chat".into(),
                "print \"{}\n\"\n".into(),
            ),
        )?
        .to_object(py),
    )?;
    m.add(
        "BLUE_TEAM_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new(
                "blue".into(),
                "blue_team_chat".into(),
                "print \"{}\n\"\n".into(),
            ),
        )?
        .to_object(py),
    )?;
    m.add(
        "FREE_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("free".into(), "free_chat".into(), "print \"{}\n\"\n".into()),
        )?
        .to_object(py),
    )?;
    m.add(
        "SPECTATOR_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new(
                "spectator".into(),
                "spectator_chat".into(),
                "print \"{}\n\"\n".into(),
            ),
        )?
        .to_object(py),
    )?;
    m.add(
        "CONSOLE_CHANNEL",
        Py::new(py, ConsoleChannel::py_new())?.to_object(py),
    )?;
    m.add("PluginLoadError", py.get_type::<PluginLoadError>())?;
    m.add("PluginUnloadError", py.get_type::<PluginUnloadError>())?;

    Ok(())
}

pub(crate) static PYSHINQLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn pyshinqlx_is_initialized() -> bool {
    PYSHINQLX_INITIALIZED.load(Ordering::SeqCst)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum PythonInitializationError {
    MainScriptError,
    #[cfg_attr(test, allow(dead_code))]
    AlreadyInitialized,
    NotInitializedError,
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
    if pyshinqlx_is_initialized() {
        error!(target: "shinqlx", "pyshinqlx_initialize was called while already initialized");
        return Err(PythonInitializationError::AlreadyInitialized);
    }

    debug!(target: "shinqlx", "Initializing Python...");
    append_to_inittab!(pyshinqlx_module);
    prepare_freethreaded_python();
    let init_result = Python::with_gil(|py| {
        let shinqlx_module = py.import("shinqlx")?;
        shinqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    });
    match init_result {
        Err(e) => {
            error!(target: "shinqlx", "{:?}", e);
            error!(target: "shinqlx", "loader sequence returned an error. Did you modify the loader?");
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(_) => {
            PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            debug!(target: "shinqlx", "Python initialized!");
            Ok(())
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyshinqlx_is_initialized() {
        error!(target: "shinqlx", "pyshinqlx_finalize was called before being initialized");
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

    let reinit_result = Python::with_gil(|py| {
        let importlib_module = py.import("importlib")?;
        let shinqlx_module = py.import("shinqlx")?;
        let new_shinqlx_module = importlib_module.call_method1("reload", (shinqlx_module,))?;
        new_shinqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    });
    match reinit_result {
        Err(_) => {
            PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(()) => {
            PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
pub(crate) mod python_tests {
    use super::PythonInitializationError;

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

    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: &str) -> Option<String> {
        None
    }

    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: &str) {}

    pub(crate) fn console_print_dispatcher(_msg: &str) -> Option<String> {
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

    pub(crate) fn pyshinqlx_is_initialized() -> bool {
        false
    }

    pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }

    pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(miri))]
pub(crate) mod pyshinqlx_setup_fixture {
    use super::pyshinqlx_module;
    use pyo3::ffi::Py_IsInitialized;
    use pyo3::{append_to_inittab, prepare_freethreaded_python};
    use rstest::fixture;

    #[fixture]
    #[once]
    pub(crate) fn pyshinqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyshinqlx_module);
            prepare_freethreaded_python();
        }
    }
}
