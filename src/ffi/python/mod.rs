mod channels;
mod commands;
mod database;
mod dispatchers;
mod embed;
mod events;
mod flight;
mod game;
mod handlers;
mod holdable;
mod player;
mod player_info;
mod player_state;
mod player_stats;
mod plugin;
mod powerups;
mod stats_listener;
mod vector3;
mod weapons;

pub(crate) mod prelude {
    pub(crate) use pyo3::prelude::*;

    #[allow(unused_imports)]
    pub(crate) use super::channels::{
        AbstractChannel, AbstractChannelMethods, ChatChannel, ChatChannelMethods,
        ClientCommandChannel, ClientCommandChannelMethods, ConsoleChannel, MAX_MSG_LENGTH,
        TeamChatChannel, TellChannel, TellChannelMethods,
    };
    #[allow(unused_imports)]
    pub(crate) use super::commands::{
        Command, CommandInvoker, CommandInvokerMethods, CommandMethods,
    };
    #[allow(unused_imports)]
    pub(crate) use super::database::{
        AbstractDatabase, AbstractDatabaseMethods, Redis, RedisMethods,
    };
    #[cfg(not(test))]
    pub(crate) use super::dispatchers::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
    };
    #[allow(unused_imports)]
    pub(crate) use super::game::{Game, GameMethods, NonexistentGameError};
    #[cfg(test)]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command, handle_console_print, handle_damage, handle_frame,
        handle_kamikaze_explode, handle_kamikaze_use, handle_new_game, handle_player_connect,
        handle_player_disconnect, handle_player_loaded, handle_player_spawn, handle_rcon,
        handle_server_command, handle_set_configstring, register_handlers,
    };
    #[cfg(test)]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command_context, handle_console_print_context, handle_damage_context,
        handle_frame_context, handle_kamikaze_explode_context, handle_kamikaze_use_context,
        handle_new_game_context, handle_player_connect_context, handle_player_disconnect_context,
        handle_player_loaded_context, handle_player_spawn_context, handle_rcon_context,
        handle_server_command_context, handle_set_configstring_context, register_handlers_context,
    };
    #[cfg(not(test))]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::{
        handle_client_command, handle_console_print, handle_damage, handle_frame,
        handle_kamikaze_explode, handle_kamikaze_use, handle_new_game, handle_player_connect,
        handle_player_disconnect, handle_player_loaded, handle_player_spawn, handle_rcon,
        handle_server_command, handle_set_configstring, register_handlers,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        client_command_dispatcher_context, client_connect_dispatcher_context,
        client_disconnect_dispatcher_context, client_loaded_dispatcher_context,
        client_spawn_dispatcher_context, console_print_dispatcher_context,
        damage_dispatcher_context, frame_dispatcher_context, kamikaze_explode_dispatcher_context,
        kamikaze_use_dispatcher_context, new_game_dispatcher_context, rcon_dispatcher_context,
        server_command_dispatcher_context, set_configstring_dispatcher_context,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize_context, pyshinqlx_is_initialized_context, pyshinqlx_reload_context,
    };
    #[allow(unused_imports)]
    pub(crate) use super::player::{
        AbstractDummyPlayer, NonexistentPlayerError, Player, PlayerMethods, RconDummyPlayer,
    };
    #[cfg(test)]
    pub(crate) use super::pyshinqlx_setup_fixture::*;
    pub(crate) use super::{
        ALLOW_FREE_CLIENT, CUSTOM_COMMAND_HANDLER, PythonInitializationError, clean_text,
        embed::*,
        events::*,
        flight::Flight,
        holdable::Holdable,
        parse_variables,
        player_info::PlayerInfo,
        player_state::PlayerState,
        player_stats::PlayerStats,
        plugin::{Plugin, PluginMethods},
        powerups::Powerups,
        stats_listener::{StatsListener, StatsListenerMethods},
        vector3::Vector3,
        weapons::Weapons,
    };
    #[cfg(not(test))]
    pub(crate) use super::{pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload};
}

use core::{
    hint::cold_path,
    ops::Deref,
    str::FromStr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use arc_swap::ArcSwapOption;
use chrono::Utc;
use commands::CommandPriorities;
use derive_more::Display;
use itertools::Itertools;
use log::*;
use prelude::*;
use pyo3::{
    append_to_inittab, create_exception,
    exceptions::{PyAttributeError, PyEnvironmentError, PyException, PyValueError},
    ffi::Py_IsInitialized,
    intern, prepare_freethreaded_python,
    types::{IntoPyDict, PyBool, PyDelta, PyDict, PyFloat, PyInt, PyString, PyTuple, PyType},
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use tap::{Conv, TapFallible, TapOptional, TryConv};

#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;
use crate::{
    _INIT_TIME, MAIN_ENGINE,
    ffi::c::prelude::*,
    quake_live_engine::{ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SetCVarForced},
};

const SHINQLX_THREADNAME: &str = "shinqlxthread";

pub(crate) static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) static CUSTOM_COMMAND_HANDLER: LazyLock<ArcSwapOption<PyObject>> =
    LazyLock::new(ArcSwapOption::empty);

pub(crate) static MODULES: LazyLock<ArcSwapOption<Py<PyDict>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static CHAT_CHANNEL: LazyLock<ArcSwapOption<Py<TeamChatChannel>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static RED_TEAM_CHAT_CHANNEL: LazyLock<ArcSwapOption<Py<TeamChatChannel>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static BLUE_TEAM_CHAT_CHANNEL: LazyLock<ArcSwapOption<Py<TeamChatChannel>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static FREE_CHAT_CHANNEL: LazyLock<ArcSwapOption<Py<TeamChatChannel>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static SPECTATOR_CHAT_CHANNEL: LazyLock<ArcSwapOption<Py<TeamChatChannel>>> =
    LazyLock::new(ArcSwapOption::empty);
pub(crate) static CONSOLE_CHANNEL: LazyLock<ArcSwapOption<Py<ConsoleChannel>>> =
    LazyLock::new(ArcSwapOption::empty);

pub(crate) static COMMANDS: LazyLock<ArcSwapOption<Py<CommandInvoker>>> =
    LazyLock::new(ArcSwapOption::empty);

pub(crate) static EVENT_DISPATCHERS: LazyLock<ArcSwapOption<Py<EventDispatcherManager>>> =
    LazyLock::new(ArcSwapOption::empty);

// Used primarily in Python, but defined here and added using PyModule_AddIntMacro().
#[allow(non_camel_case_types)]
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PythonReturnCodes {
    RET_NONE,
    RET_STOP,
    // Stop execution of event handlers within Python.
    RET_STOP_EVENT,
    // Only stop the event, but let other handlers process it.
    RET_STOP_ALL,
    // Stop execution at an engine level. SCARY STUFF!
    RET_USAGE, // Used for commands. Replies to the channel with a command's usage.
}

impl FromPyObject<'_> for PythonReturnCodes {
    fn extract_bound(item: &Bound<'_, PyAny>) -> PyResult<Self> {
        if item.is_none() {
            return Ok(PythonReturnCodes::RET_NONE);
        }
        if item.is_instance_of::<PyBool>() {
            cold_path();
            return Err(PyValueError::new_err("unsupported PythonReturnCode"));
        }
        match item.extract::<i32>() {
            Ok(ret_none) if ret_none == PythonReturnCodes::RET_NONE as i32 => {
                Ok(PythonReturnCodes::RET_NONE)
            }
            Ok(ret_stop) if ret_stop == PythonReturnCodes::RET_STOP as i32 => {
                Ok(PythonReturnCodes::RET_STOP)
            }
            Ok(ret_stop_all) if ret_stop_all == PythonReturnCodes::RET_STOP_ALL as i32 => {
                Ok(PythonReturnCodes::RET_STOP_ALL)
            }
            Ok(ret_stop_event) if ret_stop_event == PythonReturnCodes::RET_STOP_EVENT as i32 => {
                Ok(PythonReturnCodes::RET_STOP_EVENT)
            }
            Ok(ret_usage) if ret_usage == PythonReturnCodes::RET_USAGE as i32 => {
                Ok(PythonReturnCodes::RET_USAGE)
            }
            _ => {
                cold_path();
                Err(PyValueError::new_err("unsupported PythonReturnCode"))
            }
        }
    }
}

#[cfg(test)]
mod python_return_codes_tests {
    use pyo3::{
        exceptions::PyValueError,
        prelude::*,
        types::{PyBool, PyString},
    };
    use rstest::rstest;

    use super::{PythonReturnCodes, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_none(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(&py.None().into_bound(py))
                    .is_ok_and(|value| value == PythonReturnCodes::RET_NONE)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_bool_true(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(&PyBool::new(py, true))
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_bool_false(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(&PyBool::new(py, false))
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[case(0, PythonReturnCodes::RET_NONE)]
    #[case(1, PythonReturnCodes::RET_STOP)]
    #[case(2, PythonReturnCodes::RET_STOP_EVENT)]
    #[case(3, PythonReturnCodes::RET_STOP_ALL)]
    #[case(4, PythonReturnCodes::RET_USAGE)]
    #[cfg_attr(miri, ignore)]
    fn extract_from_ok_value(
        #[case] python_value: i32,
        #[case] expected_value: PythonReturnCodes,
        _pyshinqlx_setup: (),
    ) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(
                    &python_value
                        .into_pyobject(py)
                        .expect("this should not happen")
                )
                .is_ok_and(|value| value == expected_value)
            );
        });
    }

    #[rstest]
    #[case(-1)]
    #[case(5)]
    #[cfg_attr(miri, ignore)]
    fn extract_from_invalid_value(#[case] wrong_value: i32, _pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(
                    &wrong_value
                        .into_pyobject(py)
                        .expect("this should not happen")
                )
                .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_str(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert!(
                PythonReturnCodes::extract_bound(&PyString::intern(py, "asdf"))
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }
}

#[derive(Clone, Copy)]
#[repr(i32)]
enum GameTypes {
    FreeForAll = 0,
    Duel = 1,
    Race = 2,
    TeamDeathmatch = 3,
    ClanArena = 4,
    CaptureTheFlag = 5,
    OneFlag = 6,
    Harvester = 8,
    FreezeTag = 9,
    Domination = 10,
    AttackAndDefend = 11,
    RedRover = 12,
    Unknown,
}

impl GameTypes {
    pub(crate) fn type_long(&self) -> &str {
        match self {
            GameTypes::FreeForAll => "Free for All",
            GameTypes::Duel => "Duel",
            GameTypes::Race => "Race",
            GameTypes::TeamDeathmatch => "Team Deathmatch",
            GameTypes::ClanArena => "Clan Arena",
            GameTypes::CaptureTheFlag => "Capture the Flag",
            GameTypes::OneFlag => "One Flag",
            GameTypes::Harvester => "Harvester",
            GameTypes::FreezeTag => "Freeze Tag",
            GameTypes::Domination => "Domination",
            GameTypes::AttackAndDefend => "Attack and Defend",
            GameTypes::RedRover => "Red Rover",
            _ => "unknown",
        }
    }

    pub(crate) fn type_short(&self) -> &str {
        match self {
            GameTypes::FreeForAll => "ffa",
            GameTypes::Duel => "duel",
            GameTypes::Race => "race",
            GameTypes::TeamDeathmatch => "tdm",
            GameTypes::ClanArena => "ca",
            GameTypes::CaptureTheFlag => "ctf",
            GameTypes::OneFlag => "1f",
            GameTypes::Harvester => "har",
            GameTypes::FreezeTag => "ft",
            GameTypes::Domination => "dom",
            GameTypes::AttackAndDefend => "ad",
            GameTypes::RedRover => "rr",
            _ => "N/A",
        }
    }
}

impl From<i32> for GameTypes {
    fn from(value: i32) -> Self {
        match value {
            0 => GameTypes::FreeForAll,
            1 => GameTypes::Duel,
            2 => GameTypes::Race,
            3 => GameTypes::TeamDeathmatch,
            4 => GameTypes::ClanArena,
            5 => GameTypes::CaptureTheFlag,
            6 => GameTypes::OneFlag,
            8 => GameTypes::Harvester,
            9 => GameTypes::FreezeTag,
            10 => GameTypes::Domination,
            11 => GameTypes::AttackAndDefend,
            12 => GameTypes::RedRover,
            _ => GameTypes::Unknown,
        }
    }
}

#[derive(Copy, Clone, Display)]
#[repr(i32)]
enum Teams {
    #[display("free")]
    Free = team_t::TEAM_FREE as i32,
    #[display("red")]
    Red = team_t::TEAM_RED as i32,
    #[display("blue")]
    Blue = team_t::TEAM_BLUE as i32,
    #[display("spectator")]
    Spectator = team_t::TEAM_SPECTATOR as i32,
    #[display("invalid team")]
    Invalid = -1,
}

impl From<team_t> for Teams {
    fn from(value: team_t) -> Self {
        match value {
            team_t::TEAM_FREE => Teams::Free,
            team_t::TEAM_RED => Teams::Red,
            team_t::TEAM_BLUE => Teams::Blue,
            team_t::TEAM_SPECTATOR => Teams::Spectator,
            _ => Teams::Invalid,
        }
    }
}

impl From<i32> for Teams {
    fn from(value: i32) -> Self {
        match value {
            0 => Teams::Free,
            1 => Teams::Red,
            2 => Teams::Blue,
            3 => Teams::Spectator,
            _ => Teams::Invalid,
        }
    }
}

impl<'a> From<&'a str> for Teams {
    fn from(value: &'a str) -> Self {
        match value {
            "free" => Teams::Free,
            "red" => Teams::Red,
            "blue" => Teams::Blue,
            "spectator" => Teams::Spectator,
            _ => Teams::Invalid,
        }
    }
}

#[derive(Copy, Clone, Display)]
#[repr(i32)]
enum ConnectionStates {
    #[display("free")]
    Free = clientState_t::CS_FREE as i32,
    #[display("zombie")]
    Zombie = clientState_t::CS_ZOMBIE as i32,
    #[display("connected")]
    Connected = clientState_t::CS_CONNECTED as i32,
    #[display("primed")]
    Primed = clientState_t::CS_PRIMED as i32,
    #[display("active")]
    Active = clientState_t::CS_ACTIVE as i32,
}

impl From<clientState_t> for ConnectionStates {
    fn from(value: clientState_t) -> Self {
        match value {
            clientState_t::CS_FREE => ConnectionStates::Free,
            clientState_t::CS_ZOMBIE => ConnectionStates::Zombie,
            clientState_t::CS_CONNECTED => ConnectionStates::Connected,
            clientState_t::CS_PRIMED => ConnectionStates::Primed,
            clientState_t::CS_ACTIVE => ConnectionStates::Active,
        }
    }
}

#[derive(Copy, Clone, Display)]
#[repr(i32)]
enum Weapon {
    #[display("g")]
    Gauntlet = weapon_t::WP_GAUNTLET as i32,
    #[display("mg")]
    Machinegun = weapon_t::WP_MACHINEGUN as i32,
    #[display("sg")]
    Shotgun = weapon_t::WP_SHOTGUN as i32,
    #[display("gl")]
    GrenadeLauncher = weapon_t::WP_GRENADE_LAUNCHER as i32,
    #[display("rl")]
    RocketLauncher = weapon_t::WP_ROCKET_LAUNCHER as i32,
    #[display("lg")]
    Lightning = weapon_t::WP_LIGHTNING as i32,
    #[display("rg")]
    Railgun = weapon_t::WP_RAILGUN as i32,
    #[display("pg")]
    PlasmaGun = weapon_t::WP_PLASMAGUN as i32,
    #[display("bfg")]
    Bfg = weapon_t::WP_BFG as i32,
    #[display("gh")]
    GrapplingHook = weapon_t::WP_GRAPPLING_HOOK as i32,
    #[display("ng")]
    NailGun = weapon_t::WP_NAILGUN as i32,
    #[display("pl")]
    ProximityMainLauncher = weapon_t::WP_PROX_LAUNCHER as i32,
    #[display("cg")]
    ChainGun = weapon_t::WP_CHAINGUN as i32,
    #[display("hmg")]
    HeavyMachinegun = weapon_t::WP_HMG as i32,
    #[display("hands")]
    Hands = weapon_t::WP_HANDS as i32,
}

impl From<weapon_t> for Weapon {
    fn from(value: weapon_t) -> Self {
        match value {
            weapon_t::WP_GAUNTLET => Weapon::Gauntlet,
            weapon_t::WP_MACHINEGUN => Weapon::Machinegun,
            weapon_t::WP_SHOTGUN => Weapon::Shotgun,
            weapon_t::WP_GRENADE_LAUNCHER => Weapon::GrenadeLauncher,
            weapon_t::WP_ROCKET_LAUNCHER => Weapon::RocketLauncher,
            weapon_t::WP_LIGHTNING => Weapon::Lightning,
            weapon_t::WP_RAILGUN => Weapon::Railgun,
            weapon_t::WP_PLASMAGUN => Weapon::PlasmaGun,
            weapon_t::WP_BFG => Weapon::Bfg,
            weapon_t::WP_GRAPPLING_HOOK => Weapon::GrapplingHook,
            weapon_t::WP_NAILGUN => Weapon::NailGun,
            weapon_t::WP_PROX_LAUNCHER => Weapon::ProximityMainLauncher,
            weapon_t::WP_CHAINGUN => Weapon::ChainGun,
            weapon_t::WP_HMG => Weapon::HeavyMachinegun,
            _ => Weapon::Hands,
        }
    }
}

create_exception!(pyshinqlx_module, PluginLoadError, PyException);
create_exception!(pyshinqlx_module, PluginUnloadError, PyException);

pub(crate) fn clean_text<T>(text: &T) -> String
where
    T: AsRef<str>,
{
    let re = Regex::new(r#"\^[0-7]"#).unwrap();
    re.replace_all(text.as_ref(), "").to_string()
}

pub(crate) fn parse_variables(varstr: &str) -> ParsedVariables {
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
        if varstr.trim().is_empty() {
            return Ok(Self { items: vec![] });
        }

        let stripped_varstr = varstr.strip_prefix('\\').unwrap_or(varstr).to_string();

        let varstr_vec = stripped_varstr
            .split('\\')
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        if varstr_vec.len() % 2 == 1 {
            warn!(target: "shinqlx", "Uneven number of keys and values: {varstr}");
        }
        Ok(Self {
            items: varstr_vec.iter().cloned().tuples().collect(),
        })
    }
}

impl From<ParsedVariables> for String {
    fn from(value: ParsedVariables) -> Self {
        value
            .items
            .par_iter()
            .map(|(key, value)| format!(r"\{key}\{value}"))
            .collect::<Vec<_>>()
            .join("")
    }
}

impl<'py> IntoPyDict<'py> for ParsedVariables {
    fn into_py_dict(self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.items.into_py_dict(py)
    }
}

impl Deref for ParsedVariables {
    type Target = Vec<(String, String)>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl ParsedVariables {
    pub(crate) fn contains<T>(&self, item: T) -> bool
    where
        T: AsRef<str> + Send + Sync,
    {
        self.items
            .par_iter()
            .any(|(key, _value)| *key == item.as_ref())
    }

    pub fn get<T>(&self, item: T) -> Option<String>
    where
        T: AsRef<str> + Send + Sync,
    {
        self.items
            .par_iter()
            .find_any(|(key, _value)| *key == item.as_ref())
            .map(|(_key, value)| value)
            .cloned()
    }

    pub fn set(&mut self, item: &str, value: &str) {
        let mut new_items = self
            .items
            .par_iter()
            .filter(|(key, _value)| key != item)
            .cloned()
            .collect::<Vec<_>>();
        new_items.push((item.into(), value.into()));
        self.items = new_items;
    }
}

#[cfg(test)]
mod parsed_variables_tests {
    use core::str::FromStr;

    use super::ParsedVariables;

    #[test]
    fn parse_variables_with_space() {
        let variables = ParsedVariables::from_str(r"\name\Unnamed Player\country\de")
            .expect("this should not happen");
        assert!(
            variables
                .get("name")
                .is_some_and(|value| value == "Unnamed Player")
        );
        assert!(variables.get("country").is_some_and(|value| value == "de"));
    }
}

pub(crate) fn client_id(
    py: Python<'_>,
    name: &Bound<'_, PyAny>,
    player_list: Option<Vec<Player>>,
) -> Option<i32> {
    if name.is_none() {
        return None;
    }

    match name.to_string().parse::<i32>() {
        Ok(value) if (0..64).contains(&value) => Some(value),
        _ => match name.downcast::<Player>() {
            Ok(player) => Some(player.get().id),
            _ => {
                let all_players = player_list.unwrap_or_else(|| {
                    Player::all_players(&py.get_type::<Player>()).unwrap_or_default()
                });

                match name.to_string().parse::<i64>() {
                    Ok(steam_id) => all_players
                        .par_iter()
                        .find_any(|&player| player.steam_id == steam_id)
                        .map(|player| player.id),
                    _ => match name.extract::<String>() {
                        Ok(player_name) => {
                            let clean_name = clean_text(&player_name).to_lowercase();
                            all_players
                                .par_iter()
                                .find_any(|&player| {
                                    let player_name = &*player.name.read();
                                    clean_text(player_name).to_lowercase() == clean_name
                                })
                                .map(|player| player.id)
                        }
                        _ => None,
                    },
                }
            }
        },
    }
}

fn get_cvar(cvar: &str) -> PyResult<Option<String>> {
    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            Ok(main_engine
                .find_cvar(cvar)
                .map(|cvar_result| cvar_result.get_string()))
        },
    )
}

fn set_cvar(cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            main_engine.find_cvar(cvar).as_ref().map_or_else(
                || {
                    main_engine.get_cvar(cvar, value, flags);
                    Ok(true)
                },
                |_| {
                    main_engine.set_cvar_forced(
                        cvar,
                        value,
                        flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
                    );
                    Ok(false)
                },
            )
        },
    )
}

fn console_command(cmd: &str) -> PyResult<()> {
    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            main_engine.execute_console_command(cmd);
            Ok(())
        },
    )
}

fn get_configstring(index: u16) -> PyResult<String> {
    if !(0..MAX_CONFIGSTRINGS as u16).contains(&index) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| Ok(main_engine.get_configstring(index)),
    )
}

fn set_configstring(index: u16, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS as u16).contains(&index) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    shinqlx_set_configstring(index.conv::<u32>(), value);

    Ok(())
}

fn set_teamsize(value: i32) -> PyResult<()> {
    let value_str = format!("{value}");
    set_cvar("teamsize", &value_str, None).map(|_| ())
}

fn lock(team: Option<&str>) -> PyResult<()> {
    team.map_or_else(
        || console_command("lock"),
        |team_name| {
            let team_name_lower = team_name.to_lowercase();
            match Teams::from(team_name_lower.as_str()) {
                Teams::Invalid => {
                    cold_path();
                    Err(PyValueError::new_err("Invalid team."))
                }
                team => {
                    let lock_cmd = format!("lock {team}");
                    console_command(&lock_cmd)
                }
            }
        },
    )
}

fn unlock(team: Option<&str>) -> PyResult<()> {
    team.map_or_else(
        || console_command("unlock"),
        |team_name| {
            let team_name_lower = team_name.to_lowercase();
            match Teams::from(team_name_lower.as_str()) {
                Teams::Invalid => {
                    cold_path();
                    Err(PyValueError::new_err("Invalid team."))
                }
                team => {
                    let unlock_cmd = format!("unlock {team}");
                    console_command(&unlock_cmd)
                }
            }
        },
    )
}

fn put(py: Python<'_>, player: &Bound<'_, PyAny>, team: &str) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let team_lower = team.to_lowercase();
                match Teams::from(team_lower.as_str()) {
                    Teams::Invalid => {
                        cold_path();
                        Err(PyValueError::new_err("Invalid team."))
                    }
                    team => {
                        let team_change_cmd = format!("put {player_id} {team}");
                        console_command(&team_change_cmd)
                    }
                }
            })
        },
    )
}

fn mute(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let mute_cmd = format!("mute {player_id}");
                console_command(&mute_cmd)
            })
        },
    )
}

fn unmute(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let unmute_cmd = format!("unmute {player_id}");
                console_command(&unmute_cmd)
            })
        },
    )
}

fn tempban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let tempban_cmd = format!("tempban {player_id}");
                console_command(&tempban_cmd)
            })
        },
    )
}

fn ban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let ban_cmd = format!("ban {player_id}");
                console_command(&ban_cmd)
            })
        },
    )
}

fn unban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let unban_cmd = format!("unban {player_id}");
                console_command(&unban_cmd)
            })
        },
    )
}

fn opsay(msg: &str) -> PyResult<()> {
    let opsay_cmd = format!("opsay {msg}");
    console_command(&opsay_cmd)
}

fn addadmin(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let addadmin_cmd = format!("addadmin {player_id}");
                console_command(&addadmin_cmd)
            })
        },
    )
}

fn addmod(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let addmod_cmd = format!("addmod {player_id}");
                console_command(&addmod_cmd)
            })
        },
    )
}

fn demote(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let demote_cmd = format!("demote {player_id}");
                console_command(&demote_cmd)
            })
        },
    )
}

fn addscore(py: Python<'_>, player: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
    client_id(py, player, None).map_or(
        {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.allow_threads(|| {
                let addscore_cmd = format!("addscore {player_id} {score}");
                console_command(&addscore_cmd)
            })
        },
    )
}

fn addteamscore(team: &str, score: i32) -> PyResult<()> {
    let team_lower = team.to_lowercase();
    match Teams::from(team_lower.as_str()) {
        Teams::Invalid => {
            cold_path();
            Err(PyValueError::new_err("Invalid team."))
        }
        team => {
            let addteamscore_cmd = format!("addteamscore {team} {score}");
            console_command(&addteamscore_cmd)
        }
    }
}

fn is_vote_active() -> bool {
    get_configstring(CS_VOTE_STRING as u16).is_ok_and(|vote_string| !vote_string.is_empty())
}

#[pyfunction]
#[pyo3(pass_module)]
fn set_map_subtitles(module: &Bound<'_, PyModule>) -> PyResult<()> {
    get_configstring(CS_MESSAGE as u16)
        .and_then(|map_title| module.setattr(intern!(module.py(), "_map_title"), map_title))?;

    get_configstring(CS_AUTHOR as u16).and_then(|mut map_subtitle1| {
        module.setattr(intern!(module.py(), "_map_subtitle1"), &map_subtitle1)?;

        if !map_subtitle1.is_empty() {
            map_subtitle1.push_str(" - ");
        }

        map_subtitle1.push_str("Running shinqlx ^6v");
        map_subtitle1.push_str(env!("SHINQLX_VERSION"));
        map_subtitle1.push_str("^7 with plugins ^6");

        let plugins_version = module
            .getattr(intern!(module.py(), "__plugins_version__"))
            .and_then(|value| value.extract::<String>())
            .unwrap_or("NOT_SET".to_string());

        map_subtitle1.push_str(&plugins_version);
        map_subtitle1.push_str("^7.");

        set_configstring(CS_AUTHOR as u16, &map_subtitle1)
    })?;

    get_configstring(CS_AUTHOR2 as u16).and_then(|mut map_subtitle2| {
        module.setattr(intern!(module.py(), "_map_subtitle2"), &map_subtitle2)?;

        module.py().allow_threads(|| {
            if !map_subtitle2.is_empty() {
                map_subtitle2.push_str(" - ");
            }
            map_subtitle2
                .push_str("Check ^6https://github.com/mgaertne/shinqlx^7 for more details.");
            set_configstring(CS_AUTHOR2 as u16, &map_subtitle2)
        })
    })
}

#[cfg(test)]
mod set_map_subtitles_tests {
    use mockall::predicate;
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::{pyshinqlx_setup_fixture::*, set_map_subtitles};
    use crate::{
        ffi::c::prelude::{CS_AUTHOR, CS_AUTHOR2, CS_MESSAGE},
        hooks::mock_hooks::shinqlx_set_configstring_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_subtitles_sets_map_title_attribute(_pyshinqlx_setup: ()) {
        let map_subtitle1 = format!(
            "Till 'Firestarter' Merker - Running shinqlx ^6v{}^7 with plugins ^61.3.3.7^7.",
            env!("SHINQLX_VERSION")
        );
        let map_subtitle2 = "Second author would go here - Check ^6https://github.com/mgaertne/shinqlx^7 for more details.";
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR), predicate::eq(map_subtitle1))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR2), predicate::eq(map_subtitle2))
            .times(1);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "thunderstruck", 1)
            .with_get_configstring(CS_AUTHOR as u16, "Till 'Firestarter' Merker", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "Second author would go here", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    shinqlx_module
                        .setattr(intern!(py, "__plugins_version__"), "1.3.3.7")
                        .expect("this should not happen");
                    let result = set_map_subtitles(&shinqlx_module);
                    assert!(result.is_ok());
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_title"))
                            .is_ok_and(|value| value.to_string() == "thunderstruck")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle1"))
                            .is_ok_and(|value| value.to_string() == "Till 'Firestarter' Merker")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle2"))
                            .is_ok_and(|value| value.to_string() == "Second author would go here")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_subtitles_with_empty_authors(_pyshinqlx_setup: ()) {
        let map_subtitle1 = format!(
            "Running shinqlx ^6v{}^7 with plugins ^6NOT_SET^7.",
            env!("SHINQLX_VERSION")
        );
        let map_subtitle2 = "Check ^6https://github.com/mgaertne/shinqlx^7 for more details.";
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR), predicate::eq(map_subtitle1))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR2), predicate::eq(map_subtitle2))
            .times(1);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "thunderstruck", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    shinqlx_module
                        .delattr(intern!(py, "__plugins_version__"))
                        .expect("this should not happen");
                    let result = set_map_subtitles(&shinqlx_module);
                    assert!(result.is_ok());
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_title"))
                            .is_ok_and(|value| value.to_string() == "thunderstruck")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle1"))
                            .is_ok_and(|value| value.to_string() == "")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle2"))
                            .is_ok_and(|value| value.to_string() == "")
                    );
                });
            });
    }
}

/// Parses strings of key-value pairs delimited by r"\" and puts
/// them into a dictionary.
#[pyfunction]
#[pyo3(name = "parse_variables")]
#[pyo3(signature = (varstr, ordered = false), text_signature = "(varstr, ordered = false)")]
fn pyshinqlx_parse_variables<'py>(
    py: Python<'py>,
    varstr: &str,
    #[allow(unused_variables)] ordered: bool,
) -> Bound<'py, PyDict> {
    let parsed_variables = py.allow_threads(|| parse_variables(varstr));
    parsed_variables.into_py_dict(py).unwrap_or(PyDict::new(py))
}

#[cfg(test)]
mod pyshinqlx_parse_variables_test {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::{pyshinqlx_parse_variables, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn parse_variables_with_space(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let variables = pyshinqlx_parse_variables(py, r"\name\Unnamed Player\country\de", true);
            assert!(variables.get_item(intern!(py, "name")).is_ok_and(|value| {
                value.is_some_and(|str_value| str_value.to_string() == "Unnamed Player")
            }));
            assert!(variables
                .get_item(intern!(py, "country"))
                .is_ok_and(|value| value.is_some_and(|str_value| str_value.to_string() == "de")));
        });
    }
}

fn get_logger_name(py: Python<'_>, plugin: Option<Bound<'_, PyAny>>) -> String {
    let opt_plugin_name = plugin.and_then(|req_plugin| {
        req_plugin
            .str()
            .ok()
            .map(|plugin_name| plugin_name.to_string())
    });
    py.allow_threads(|| {
        opt_plugin_name.map_or("shinqlx".to_string(), |plugin_name| {
            format!("shinqlx.{plugin_name}")
        })
    })
}

/// Provides a logger that should be used by your plugin for debugging, info and error reporting. It will automatically output to both the server console as well as to a file.
#[pyfunction]
#[pyo3(name = "get_logger")]
#[pyo3(signature = (plugin = None), text_signature = "(plugin = None)")]
pub(crate) fn pyshinqlx_get_logger<'py>(
    py: Python<'py>,
    plugin: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let logger_name = get_logger_name(py, plugin);
    PyModule::import(py, intern!(py, "logging")).and_then(|logging_module| {
        logging_module.call_method1(intern!(py, "getLogger"), (logger_name,))
    })
}

#[pyfunction]
#[pyo3(name = "_configure_logger")]
fn pyshinqlx_configure_logger(py: Python<'_>) -> PyResult<()> {
    let (homepath, num_max_logs, max_logssize) = MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err("no main engine found"))
        },
        |main_engine| {
            let homepath = main_engine
                .find_cvar("fs_homepath")
                .map(|homepath_cvar| homepath_cvar.get_string().to_string())
                .unwrap_or(".".into());
            let num_max_logs = main_engine
                .find_cvar("qlx_logs")
                .map(|max_logs_cvar| max_logs_cvar.get_integer())
                .unwrap_or_default();
            let max_logsize = main_engine
                .find_cvar("qlx_logsSize")
                .map(|max_logsize_cvar| max_logsize_cvar.get_integer())
                .unwrap_or_default();
            Ok((homepath, num_max_logs, max_logsize))
        },
    )?;

    py.import(intern!(py, "logging")).and_then(|logging_module| {
        let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
        let info_level = logging_module.getattr(intern!(py, "INFO"))?;
        logging_module
            .call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))
            .and_then(|shinqlx_logger| {
                shinqlx_logger.call_method1(intern!(py, "setLevel"), (&debug_level,))?;
                let console_fmt = logging_module.call_method1(
                    intern!(py, "Formatter"),
                    (
                        "[%(name)s.%(funcName)s] %(levelname)s: %(message)s",
                        "%H:%M:%S",
                    ),
                )?;

                logging_module
                    .call_method0(intern!(py, "StreamHandler"))
                    .and_then(|console_handler| {
                        console_handler.call_method1(intern!(py, "setLevel"), (&info_level,))?;
                        console_handler.call_method1(intern!(py, "setFormatter"), (console_fmt,))?;
                        shinqlx_logger.call_method1(intern!(py, "addHandler"), (console_handler,))
                    })?;

                let file_fmt = logging_module.call_method1(
                    intern!(py, "Formatter"),
                    (
                        "(%(asctime)s) [%(levelname)s @ %(name)s.%(funcName)s] %(message)s",
                        "%H:%M:%S",
                    ),
                )?;
                py.import(intern!(py, "logging.handlers")).and_then(|handlers_submodule| {
                    let py_max_logssize = PyInt::new(py, max_logssize).into_any();
                    let py_num_max_logs = PyInt::new(py, num_max_logs).into_any();
                    handlers_submodule
                        .call_method(
                            intern!(py, "RotatingFileHandler"),
                            (format!("{homepath}/shinqlx.log"),),
                            Some(
                                &[
                                    ("encoding", PyString::intern(py, "utf-8").into_any()),
                                    ("maxBytes", py_max_logssize),
                                    ("backupCount", py_num_max_logs),
                                ]
                                .into_py_dict(py)?,
                            ),
                        )
                        .and_then(|file_handler| {
                            file_handler.call_method1(intern!(py, "setLevel"), (&debug_level,))?;
                            file_handler.call_method1(intern!(py, "setFormatter"), (file_fmt,))?;
                            shinqlx_logger.call_method1(intern!(py, "addHandler"), (file_handler,))
                        })
                })?;

                let datetime_now = py.import(intern!(py, "datetime")).and_then(|datetime_module| {
                    datetime_module
                        .getattr(intern!(py, "datetime"))
                        .and_then(|datetime_object| datetime_object.call_method0(intern!(py, "now")))
                })?;
                shinqlx_logger
                    .call_method1(
                        intern!(py, "info"),
                        (
                            "============================= shinqlx run @ %s =============================",
                            datetime_now,
                        ),
                    )
                    .map(|_| ())
        })
    })
}

#[cfg(test)]
mod pyshinqlx_configure_logger_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::{pyshinqlx_configure_logger, pyshinqlx_setup_fixture::*};
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    fn clear_logger(py: Python<'_>) {
        let logging_module = py
            .import(intern!(py, "logging"))
            .expect("this should not happen");
        let shinqlx_logger = logging_module
            .call_method1(intern!(py, "getLogger"), ("shinqlx",))
            .expect("this should not happen");
        while shinqlx_logger
            .call_method0(intern!(py, "hasHandlers"))
            .is_ok_and(|value| {
                value
                    .downcast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            })
        {
            let shinqlx_handlers = shinqlx_logger
                .getattr(intern!(py, "handlers"))
                .expect("this should not happen");
            let _ = shinqlx_logger.call_method1(
                intern!(py, "removeHandler"),
                (shinqlx_handlers
                    .get_item(0)
                    .expect("this should not happen"),),
            );
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_configure_logger(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    static TEMP_LOG_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_cvars_in_main_engine(_pyshinqlx_setup: ()) {
        let cvar_string = CString::new(TEMP_LOG_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let mut raw_homepath_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_logs_cvar = CVarBuilder::default()
            .integer(10)
            .build()
            .expect("this should not happen");
        let mut raw_logssize_cvar = CVarBuilder::default()
            .integer(1024)
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "fs_homepath",
                move |_| CVar::try_from(raw_homepath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_logs",
                move |_| CVar::try_from(raw_logs_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_logsSize",
                move |_| CVar::try_from(raw_logssize_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok());

                    let logging_module = py
                        .import(intern!(py, "logging"))
                        .expect("this should not happen");
                    let debug_level = logging_module
                        .getattr(intern!(py, "DEBUG"))
                        .expect("this should not happen");
                    let shinqlx_logger = logging_module
                        .call_method1(intern!(py, "getLogger"), ("shinqlx",))
                        .expect("this should not happen");
                    let log_level = shinqlx_logger.getattr(intern!(py, "level"));
                    assert!(log_level.is_ok_and(|value| {
                        value.eq(&debug_level).expect("this should not happen")
                    }),);

                    let info_level = logging_module
                        .getattr(intern!(py, "INFO"))
                        .expect("this should not happen");
                    let shinqlx_handlers = shinqlx_logger
                        .getattr(intern!(py, "handlers"))
                        .expect("this should not happen");
                    let logging_stream_handler = logging_module
                        .getattr(intern!(py, "StreamHandler"))
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(0).is_ok_and(|first_handler| {
                        first_handler
                            .is_instance(&logging_stream_handler)
                            .expect("this should not happen")
                            && first_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&info_level).expect("this should not happen")
                                })
                    }));
                    let rotating_file_handler = logging_module
                        .getattr(intern!(py, "handlers"))
                        .and_then(|handlers_submodule| {
                            handlers_submodule.getattr(intern!(py, "RotatingFileHandler"))
                        })
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(1).is_ok_and(|second_handler| {
                        second_handler
                            .is_instance(&rotating_file_handler)
                            .expect("this should not happen")
                            && second_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&debug_level).expect("this should not happen")
                                })
                    }));

                    clear_logger(py);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_no_cvars_configured_in_main_engine(_pyshinqlx_setup: ()) {
        let cvar_string = CString::new(TEMP_LOG_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let mut raw_homepath_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "fs_homepath",
                move |_| CVar::try_from(raw_homepath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(|cmd| cmd == "qlx_logs", |_| None, 1)
            .with_find_cvar(|cmd| cmd == "qlx_logsSize", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok());

                    let logging_module = py
                        .import(intern!(py, "logging"))
                        .expect("this should not happen");
                    let debug_level = logging_module
                        .getattr(intern!(py, "DEBUG"))
                        .expect("this should not happen");
                    let shinqlx_logger = logging_module
                        .call_method1(intern!(py, "getLogger"), ("shinqlx",))
                        .expect("this should not happen");
                    let log_level = shinqlx_logger.getattr(intern!(py, "level"));
                    assert!(log_level.is_ok_and(|value| {
                        value.eq(&debug_level).expect("this should not happen")
                    }),);

                    let info_level = logging_module
                        .getattr(intern!(py, "INFO"))
                        .expect("this should not happen");
                    let shinqlx_handlers = shinqlx_logger
                        .getattr(intern!(py, "handlers"))
                        .expect("this should not happen");
                    let logging_stream_handler = logging_module
                        .getattr(intern!(py, "StreamHandler"))
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(0).is_ok_and(|first_handler| {
                        first_handler
                            .is_instance(&logging_stream_handler)
                            .expect("this should not happen")
                            && first_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&info_level).expect("this should not happen")
                                })
                    }));
                    let rotating_file_handler = logging_module
                        .getattr(intern!(py, "handlers"))
                        .and_then(|handlers_submodule| {
                            handlers_submodule.getattr(intern!(py, "RotatingFileHandler"))
                        })
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(1).is_ok_and(|second_handler| {
                        second_handler
                            .is_instance(&rotating_file_handler)
                            .expect("this should not happen")
                            && second_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&debug_level).expect("this should not happen")
                                })
                    }));

                    clear_logger(py);
                });
            });
    }
}

/// Logs an exception using :func:`get_logger`. Call this in an except block.
#[pyfunction]
#[pyo3(name = "log_exception")]
#[pyo3(signature = (plugin = None), text_signature = "(plugin = None)")]
fn pyshinqlx_log_exception(py: Python<'_>, plugin: Option<Bound<'_, PyAny>>) -> PyResult<()> {
    py.import(intern!(py, "sys")).and_then(|sys_module| {
        sys_module
            .call_method0(intern!(py, "exc_info"))
            .and_then(|exc_info| {
                py.import(intern!(py, "traceback"))
                    .and_then(|traceback_module| {
                        traceback_module
                            .call_method1(
                                intern!(py, "format_exception"),
                                exc_info.downcast::<PyTuple>()?.to_owned(),
                            )
                            .and_then(|value| {
                                value
                                    .extract::<Vec<String>>()
                                    .and_then(|formatted_traceback| {
                                        try_log_messages(
                                            py,
                                            plugin,
                                            intern!(py, "log_exception"),
                                            formatted_traceback,
                                        )
                                    })
                            })
                    })
            })
    })
}

#[cfg(test)]
mod pyshinqlx_log_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::{pyshinqlx_log_exception, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn log_exception_with_no_exception_pending(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_log_exception(py, None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn log_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

try:
    raise ValueError("asdf")
except ValueError:
    shinqlx.log_exception()
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

/// A handler for unhandled exceptions.
#[pyfunction]
#[pyo3(name = "handle_exception")]
fn pyshinqlx_handle_exception(
    py: Python<'_>,
    exc_type: Bound<'_, PyAny>,
    exc_value: Bound<'_, PyAny>,
    exc_traceback: Bound<'_, PyAny>,
) -> PyResult<()> {
    py.import(intern!(py, "traceback"))
        .and_then(|traceback_module| {
            traceback_module
                .call_method1(
                    intern!(py, "format_exception"),
                    (exc_type, exc_value, exc_traceback),
                )
                .and_then(|value| {
                    value
                        .extract::<Vec<String>>()
                        .and_then(|formatted_traceback| {
                            try_log_messages(
                                py,
                                None,
                                intern!(py, "log_exception"),
                                formatted_traceback,
                            )
                        })
                })
        })
}

#[cfg(test)]
mod pyshinqlx_handle_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::{pyshinqlx_handle_exception, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_exception_with_no_exception_pending(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_handle_exception(
                py,
                py.None().into_bound(py),
                py.None().into_bound(py),
                py.None().into_bound(py),
            );
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

try:
    raise ValueError("asdf")
except ValueError as e:
    shinqlx.handle_exception(type(e), e, e.__traceback__)
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

#[pyfunction]
#[pyo3(name = "threading_excepthook")]
fn pyshinqlx_handle_threading_exception(py: Python<'_>, args: Bound<'_, PyAny>) -> PyResult<()> {
    pyshinqlx_handle_exception(
        py,
        args.getattr(intern!(py, "exc_type"))?,
        args.getattr(intern!(py, "exc_value"))?,
        args.getattr(intern!(py, "exc_traceback"))?,
    )
}

#[cfg(test)]
mod pyshinqlx_handle_threading_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::pyshinqlx_setup_fixture::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_threading_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

class ExceptHookArgs:
    def __init__(self, exc_type, exc_value, exc_traceback):
        self.exc_type = exc_type
        self.exc_value = exc_value
        self.exc_traceback = exc_traceback

try:
    raise ValueError("asdf")
except ValueError as e:
    shinqlx.threading_excepthook(ExceptHookArgs(type(e), e, e.__traceback__))
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

fn try_log_exception(py: Python<'_>, exception: &PyErr) -> PyResult<()> {
    py.import(intern!(py, "traceback"))
        .and_then(|traceback_module| {
            traceback_module
                .call_method1(
                    intern!(py, "format_exception"),
                    (
                        exception.get_type(py),
                        exception.value(py),
                        exception.traceback(py),
                    ),
                )?
                .extract::<Vec<String>>()
                .and_then(|formatted_traceback| {
                    try_log_messages(py, None, intern!(py, "log_exception"), formatted_traceback)
                })
        })
}

pub(crate) fn log_exception(py: Python<'_>, exception: &PyErr) {
    let _ = try_log_exception(py, exception);
}

fn try_log_messages(
    py: Python<'_>,
    plugin: Option<Bound<'_, PyAny>>,
    function: &Bound<'_, PyString>,
    messages: Vec<String>,
) -> Result<(), PyErr> {
    py.import(intern!(py, "logging"))
        .and_then(|logging_module| {
            let error_level = logging_module.getattr(intern!(py, "ERROR"))?;
            let logger_name = get_logger_name(py, plugin);
            logging_module
                .call_method1(intern!(py, "getLogger"), (logger_name,))
                .and_then(|py_logger| {
                    for line in messages {
                        py_logger
                            .call_method(
                                intern!(py, "makeRecord"),
                                (
                                    intern!(py, "shinqlx"),
                                    &error_level,
                                    intern!(py, ""),
                                    -1,
                                    line.trim_end(),
                                    py.None(),
                                    py.None(),
                                ),
                                Some(&[(intern!(py, "func"), function)].into_py_dict(py)?),
                            )
                            .and_then(|log_record| {
                                py_logger.call_method1(intern!(py, "handle"), (log_record,))
                            })?;
                    }
                    Ok(())
                })
        })
}

#[pyfunction]
fn next_frame<'py>(py: Python<'py>, func: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if !func.is_callable() {
        cold_path();
        return Err(PyValueError::new_err(
            "func has to be a callable Python function",
        ));
    }

    PyModule::from_code(
        py,
        cr#"
from functools import wraps

import shinqlx


def next_frame(func):
    @wraps(func)
    def f(*args, **kwargs):
        shinqlx.next_frame_tasks.put_nowait((func, args, kwargs))

    return f
        "#,
        c"",
        c"",
    )
    .and_then(|next_frame_def| next_frame_def.getattr(intern!(py, "next_frame")))
    .and_then(|next_frame_func| next_frame_func.call1((func,)))
}

#[cfg(test)]
mod next_frame_tests {
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::pyshinqlx_setup_fixture::*;
    use crate::{ffi::python::pyshinqlx_test_support::run_all_frame_tasks, prelude::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_enqueues_function_call(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.next_frame
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "get_nowait"))
                    .is_ok()
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_enqueues_function_call_on_an_instance_method(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

class TestClass:
    @shinqlx.next_frame
    def test_func(self):
        pass

TestClass().test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let result2 = run_all_frame_tasks(py);
            assert!(result2.is_ok());
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }
}

/// Delay a function call a certain amount of time.
///
///     .. note::
///         It cannot guarantee you that it will be called right as the timer
///         expires, but unless some plugin is for some reason blocking, then
///         you can expect it to be called practically as soon as it expires.
#[pyfunction]
fn delay<'py>(py: Python<'py>, time: f64) -> PyResult<Bound<'py, PyAny>> {
    PyModule::from_code(
        py,
        cr#"
from functools import wraps

import shinqlx


def delay(time):
    def wrap(func):
        @wraps(func)
        def f(*args, **kwargs):
            shinqlx.frame_tasks.enter(time, 1, func, args, kwargs)

        if not callable(func):
            raise ValueError("'func' must be callable")

        return f

    return wrap
    "#,
        c"",
        c"",
    )
    .and_then(|delay_def| delay_def.getattr(intern!(py, "delay")))
    .and_then(|delay_func| delay_func.call1((PyFloat::new(py, time),)))
}

#[cfg(test)]
mod delay_tests {
    use pretty_assertions::assert_eq;
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::pyshinqlx_setup_fixture::*;
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_enqueues_function_call(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.delay(10)
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            let scheduler_queue = frame_tasks
                .getattr(intern!(py, "queue"))
                .expect("this should not happen");
            assert_eq!(scheduler_queue.len().expect("this should not happen"), 1);
            let queue_entry = scheduler_queue.get_item(0).expect("this should not happen");
            let _ = frame_tasks.call_method1(intern!(py, "cancel"), (queue_entry,));
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_with_float_delay(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.delay(5.5)
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            let scheduler_queue = frame_tasks
                .getattr(intern!(py, "queue"))
                .expect("this should not happen");
            assert_eq!(scheduler_queue.len().expect("this should not happen"), 1);
            let queue_entry = scheduler_queue.get_item(0).expect("this should not happen");
            let _ = frame_tasks.call_method1(intern!(py, "cancel"), (queue_entry,));
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_enqueues_function_call_for_class_method(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx

class TestClass:
    @shinqlx.delay(0.0001)
    def test_func(self):
        pass

TestClass().test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let frame_tasks = py
                .import(intern!(py, "shinqlx"))
                .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "frame_tasks")))
                .expect("this should not happen");
            let sched_result = frame_tasks.call_method0(intern!(py, "run"));
            assert!(sched_result.is_ok());
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }
}

/// Starts a thread with the function passed as its target. If a function decorated
/// with this is called within a function also decorated, it will **not** create a second
/// thread unless told to do so with the *force* keyword.
#[pyfunction]
fn thread<'py>(py: Python<'py>, func: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if !func.is_callable() {
        cold_path();
        return Err(PyValueError::new_err(
            "func has to be a callable Python function",
        ));
    }

    PyModule::from_code(
        py,
        cr#"
import threading
from functools import wraps

import shinqlx


def thread(func):
    @wraps(func)
    def f(*args, **kwargs):
        if threading.current_thread().name.endswith(shinqlx._thread_name):
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
        c"",
        c"",
    )
    .and_then(|thread_def| thread_def.getattr(intern!(py, "thread")))
    .and_then(|thread_func| thread_func.call1((func,)))
}

#[cfg(test)]
mod thread_tests {
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::pyshinqlx_setup_fixture::*;
    use crate::prelude::serial;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_on_callable_function(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let py_test_module = PyModule::from_code(
                py,
                cr#"
import shinqlx

called = False

@shinqlx.thread
def threaded_func():
    global called
    called = True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");

            let result = py_test_module.call_method0(intern!(py, "threaded_func"));
            assert!(result.is_ok());

            assert!(
                py_test_module
                    .getattr(intern!(py, "called"))
                    .is_ok_and(|called| {
                        called
                            .downcast::<PyBool>()
                            .is_ok_and(|pybool| pybool.is_true())
                    })
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_on_nested_thread(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let py_test_module = PyModule::from_code(
                py,
                cr#"
import shinqlx

called = False

@shinqlx.thread
def inner_thread():
    global called
    called = True

@shinqlx.thread
def threaded_func():
    inner_thread()
"#,
                c"",
                c"",
            )
            .expect("this should not happen");

            let result = py_test_module.call_method0(intern!(py, "threaded_func"));
            assert!(result.is_ok());

            assert!(
                py_test_module
                    .getattr(intern!(py, "called"))
                    .is_ok_and(|called| {
                        called
                            .downcast::<PyBool>()
                            .is_ok_and(|pybool| pybool.is_true())
                    })
            );
        });
    }
}

/// Returns a :class:`datetime.timedelta` instance of the time since initialized.
#[pyfunction]
fn uptime(py: Python<'_>) -> PyResult<Bound<'_, PyDelta>> {
    let (elapsed_days, elapsed_seconds, elapsed_microseconds) = py.allow_threads(|| {
        let elapsed = Utc::now() - *_INIT_TIME;
        let elapsed_days = elapsed.num_days().try_conv::<i32>().unwrap_or_default();
        let elapsed_seconds = (elapsed.num_seconds() % (24 * 60 * 60))
            .try_conv::<i32>()
            .unwrap_or_default();
        let elapsed_microseconds = elapsed.subsec_micros();
        (elapsed_days, elapsed_seconds, elapsed_microseconds)
    });
    PyDelta::new(
        py,
        elapsed_days,
        elapsed_seconds,
        elapsed_microseconds,
        false,
    )
}

#[cfg(test)]
mod uptime_tests {
    use pyo3::{prelude::*, types::PyDeltaAccess};
    use rstest::rstest;

    use super::{pyshinqlx_setup_fixture::*, uptime};
    use crate::{_INIT_TIME, prelude::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn uptime_returns_uptime(_pyshinqlx_setup: ()) {
        let _ = _INIT_TIME.timestamp();

        Python::with_gil(|py| {
            let result = uptime(py);
            assert!(
                result.is_ok_and(|time_delta| time_delta.get_microseconds() > 0
                    || time_delta.get_seconds() > 0
                    || time_delta.get_days() > 0)
            );
        });
    }
}

/// Returns the SteamID64 of the owner. This is set in the config.
#[pyfunction(name = "owner")]
fn pyshinqlx_owner(py: Python<'_>) -> PyResult<Option<i64>> {
    py.allow_threads(owner)
}

fn owner() -> PyResult<Option<i64>> {
    get_cvar("qlx_owner").map(|opt_value| {
        opt_value.and_then(|value| value.parse::<i64>().ok()).filter(|&int_value| int_value >= 0).tap_none(|| {
            cold_path();
            error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
        })
    })
}

#[cfg(test)]
mod owner_tests {
    use core::borrow::BorrowMut;

    use pyo3::{exceptions::PyEnvironmentError, prelude::*};
    use rstest::rstest;

    use super::{pyshinqlx_owner, pyshinqlx_setup_fixture::*};
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn owner_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_owner(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn owner_with_no_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_owner(py);
                    assert!(result.is_ok_and(|opt_value| opt_value.is_none()));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn owner_with_unparseable_cvar(_pyshinqlx_setup: ()) {
        let cvar_string = c"asdf";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_owner(py);
                    assert!(result.is_ok_and(|opt_value| opt_value.is_none()));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn owner_with_negative_cvar(_pyshinqlx_setup: ()) {
        let cvar_string = c"-42";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_owner(py);
                    assert!(result.is_ok_and(|opt_value| opt_value.is_none()));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn owner_with_parseable_cvar(_pyshinqlx_setup: ()) {
        let cvar_string = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let result = pyshinqlx_owner(py);
                    assert!(
                        result.is_ok_and(
                            |opt_value| opt_value.is_some_and(|value| value == 1234567890)
                        )
                    );
                });
            });
    }
}

/// Returns the :class:`shinqlx.StatsListener` instance used to listen for stats.
#[pyfunction(name = "stats_listener")]
#[pyo3(pass_module)]
fn get_stats_listener<'py>(module: &Bound<'py, PyModule>) -> PyResult<Bound<'py, PyAny>> {
    module.getattr(intern!(module.py(), "_stats"))
}

#[cfg(test)]
mod stats_listener_tests {
    use core::borrow::BorrowMut;

    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::{get_stats_listener, pyshinqlx_setup_fixture::*, stats_listener::StatsListener};
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_stats_listener_returns_none(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            shinqlx_module
                .setattr(intern!(py, "_stats"), py.None())
                .expect("this should not happen");

            let result = get_stats_listener(&shinqlx_module);
            assert!(result.is_ok_and(|value| value.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_stats_listener_returns_stats_listener(_pyshinqlx_setup: ()) {
        let cvar_string = c"0";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .integer(0)
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    let stats_listener = StatsListener::py_new().expect("this should not happen");
                    shinqlx_module
                        .setattr(
                            intern!(py, "_stats"),
                            Bound::new(py, stats_listener).expect("this should not happen"),
                        )
                        .expect("this should not happen");

                    let result = get_stats_listener(&shinqlx_module);
                    assert!(result.is_ok_and(|value| value.is_instance_of::<StatsListener>()));
                });
            });
    }
}

fn try_get_plugins_version(path: &str) -> Result<String, git2::Error> {
    git2::Repository::open(path).and_then(|repository| {
        let mut describe_options_binding = git2::DescribeOptions::default();
        let describe_options = describe_options_binding
            .describe_tags()
            .show_commit_oid_as_fallback(true);

        let plugins_version = repository.describe(describe_options).and_then(|describe| {
            let mut describe_format_options_binding = git2::DescribeFormatOptions::default();
            let desribe_format_options = describe_format_options_binding
                .abbreviated_size(8)
                .always_use_long_format(true)
                .dirty_suffix("-dirty");

            describe.format(Some(desribe_format_options))
        })?;

        repository
            .revparse_ext("HEAD")
            .ok()
            .and_then(|(_, branch_option)| branch_option)
            .map_or(Ok(plugins_version.to_owned()), |branch| {
                branch
                    .shorthand()
                    .map_or(Ok(plugins_version.to_owned()), |branch_name| {
                        let returned = format!("{}-{}", &plugins_version, branch_name);
                        Ok(returned)
                    })
            })
    })
}

fn get_plugins_version(path: &str) -> String {
    try_get_plugins_version(path).unwrap_or("NOT_SET".to_string())
}

#[pyfunction(name = "set_plugins_version", pass_module)]
fn set_plugins_version(module: &Bound<'_, PyModule>, path: &str) {
    let plugins_version = module.py().allow_threads(|| get_plugins_version(path));

    let _ = module.setattr(intern!(module.py(), "__plugins_version__"), plugins_version);
}

#[cfg(test)]
mod plugins_version_tests {
    use pretty_assertions::assert_eq;

    use super::get_plugins_version;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_plugins_version_of_folder_not_in_version_control() {
        let tempdir = tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen");

        let result = get_plugins_version(&tempdir.path().to_string_lossy());
        assert_eq!(result, "NOT_SET");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_plugins_version_of_empty_folder_in_version_control() {
        let tempdir = tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen");

        git2::Repository::init(&tempdir).expect("this should not happen");

        let result = get_plugins_version(&tempdir.path().to_string_lossy());
        assert_eq!(result, "NOT_SET");
    }

    fn create_initial_commit(repo: &git2::Repository) -> Result<(), git2::Error> {
        let signature = git2::Signature::now("testUser", "test@me.com")?;
        let oid = repo.index()?.write_tree()?;
        let tree = repo.find_tree(oid)?;
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial commit",
            &tree,
            &[],
        )
        .map(|_| ())
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_plugins_version_of_empty_folder_in_version_control_with_initial_commit_oid_fallback() {
        let tempdir = tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen");

        git2::Repository::init(&tempdir)
            .and_then(|repository| create_initial_commit(&repository))
            .expect("this should not happen");

        let result = get_plugins_version(&tempdir.path().to_string_lossy());
        assert_eq!(result.len(), 15);
        assert!(result.ends_with("-master"));
    }
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

#[pyfunction(name = "load_preset_plugins")]
fn load_preset_plugins(py: Python<'_>) -> PyResult<()> {
    try_get_plugins_path().map_err(PluginLoadError::new_err)?;

    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PluginLoadError::new_err("no main engine found"))
        },
        |main_engine| {
            main_engine
                .find_cvar("qlx_plugins")
                .tap_some(|plugins_cvar| {
                    let plugins_str = plugins_cvar.get_string();
                    let mut plugins = plugins_str
                        .split(',')
                        .map(|value| value.trim())
                        .collect::<Vec<_>>();
                    if plugins.contains(&"DEFAULT") {
                        plugins.extend_from_slice(&DEFAULT_PLUGINS);
                        plugins.retain(|&value| value != "DEFAULT");
                    }
                    plugins.iter().unique().for_each(|&plugin| {
                        let _ = load_plugin(py, plugin);
                    });
                });

            Ok(())
        },
    )
}

fn try_load_plugin(py: Python<'_>, plugin: &str, plugins_path: &Path) -> PyResult<()> {
    let Some(plugins_dir) = plugins_path
        .file_name()
        .map(|plugins_dir| plugins_dir.to_string_lossy())
    else {
        cold_path();
        return Err(PluginLoadError::new_err(
            "qlx_pluginsPath is not pointing to an existing directory",
        ));
    };

    let plugin_import_path = format!("{}.{}", plugins_dir, &plugin);
    let module = py
        .import(intern!(py, "importlib"))
        .and_then(|importlib_module| {
            importlib_module.call_method1(intern!(py, "import_module"), (plugin_import_path,))
        })?;

    let plugin_pystring = PyString::intern(py, plugin);
    if let Some(modules) = MODULES.load().as_ref() {
        modules.bind(py).set_item(&plugin_pystring, &module)?;
    }

    if !module.hasattr(&plugin_pystring)? {
        cold_path();
        return Err(PluginLoadError::new_err(
            "The plugin needs to have a class with the exact name as the file, minus the .py.",
        ));
    }

    let plugin_class = module.getattr(&plugin_pystring)?;
    let plugin_type = py.get_type::<Plugin>();
    if !plugin_class
        .downcast::<PyType>()?
        .is_subclass(&plugin_type)
        .unwrap_or(false)
    {
        cold_path();
        return Err(PluginLoadError::new_err(
            "Attempted to load a plugin that is not a subclass of 'shinqlx.Plugin'.",
        ));
    }

    plugin_class.call0().and_then(|loaded_plugin| {
        let loaded_plugins = py
            .get_type::<Plugin>()
            .getattr(intern!(py, "_loaded_plugins"))?;
        loaded_plugins
            .downcast::<PyDict>()?
            .set_item(plugin, loaded_plugin)
    })
}

#[pyfunction(name = "load_plugin")]
fn load_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let info_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "INFO")))?;
        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    &info_level,
                    intern!(py, ""),
                    -1,
                    intern!(py, "Loading plugin '%s'..."),
                    (plugin,),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "load_plugin"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    try_get_plugins_path().map_or_else(
        |err| {
            cold_path();
            Err(PluginLoadError::new_err(err))
        },
        |plugins_path| {
            let mut joined_path = plugins_path.to_owned();
            joined_path.push(plugin);
            joined_path.set_extension("py");

            if !joined_path.as_path().is_file() {
                cold_path();
                return Err(PluginLoadError::new_err("No such plugin exists."));
            }

            let loaded_plugins = py
                .get_type::<Plugin>()
                .getattr(intern!(py, "_loaded_plugins"))?;

            if loaded_plugins.downcast::<PyDict>()?.contains(plugin)? {
                return reload_plugin(py, plugin);
            }

            try_load_plugin(py, plugin, plugins_path.as_path()).tap_err(|e| {
                log_exception(py, e);
            })
        },
    )
}

fn try_unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "unload"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to unload dispatcher",
                ))
            },
            |unload_dispatcher| {
                UnloadDispatcherMethods::dispatch(
                    unload_dispatcher.downcast()?,
                    PyString::intern(py, plugin).as_any(),
                )
            },
        )?;

    let loaded_plugins = py
        .get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?;

    loaded_plugins
        .downcast::<PyDict>()?
        .get_item(plugin)?
        .map_or_else(
            || {
                cold_path();
                let error_msg =
                    format!("Attempted to unload a plugin '{plugin}' that is not loaded.");
                Err(PluginUnloadError::new_err(error_msg))
            },
            |loaded_plugin| {
                loaded_plugin
                    .getattr(intern!(py, "hooks"))?
                    .try_iter()?
                    .flatten()
                    .for_each(|hook| {
                        let _ = hook
                            .downcast_into::<PyTuple>()
                            .map_err(PyErr::from)
                            .and_then(|hook_tuple| {
                                let event_name = hook_tuple.get_item(0)?;
                                let event_handler = hook_tuple.get_item(1)?;
                                let event_priority = hook_tuple.get_item(2)?;
                                EVENT_DISPATCHERS
                                    .load()
                                    .as_ref()
                                    .and_then(|event_dispatchers| {
                                        event_dispatchers.bind(py).get_item(&event_name).ok()
                                    })
                                    .map_or(
                                        {
                                            cold_path();
                                            Err(PyAttributeError::new_err(event_name.to_string()))
                                        },
                                        |event_dispatcher| {
                                            let plugin_name =
                                                loaded_plugin.getattr(intern!(py, "name"))?;

                                            event_dispatcher
                                                .call_method1(
                                                    intern!(py, "remove_hook"),
                                                    (plugin_name, event_handler, event_priority),
                                                )
                                                .map(|_| ())
                                        },
                                    )
                            })
                            .tap_err(|err| {
                                log_exception(py, err);
                            });
                    });

                loaded_plugin
                    .getattr(intern!(py, "commands"))?
                    .try_iter()?
                    .flatten()
                    .for_each(|cmd| {
                        let _ = COMMANDS
                            .load()
                            .as_ref()
                            .map_or(
                                {
                                    cold_path();
                                    Err(PyEnvironmentError::new_err(
                                        "could not get access to commands",
                                    ))
                                },
                                |commands| commands.bind(py).remove_command(cmd.downcast()?),
                            )
                            .tap_err(|err| {
                                log_exception(py, err);
                            });
                    });

                loaded_plugins.del_item(plugin)
            },
        )
}

#[pyfunction(name = "unload_plugin")]
fn unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let info_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "INFO")))?;

        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    &info_level,
                    intern!(py, ""),
                    -1,
                    intern!(py, "Unloading plugin '%s'..."),
                    (plugin,),
                    py.None(),
                ),
                Some(&[(intern!(py, "func"), intern!(py, "unload_plugin"))].into_py_dict(py)?),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    let loaded_plugins = py
        .get_type::<Plugin>()
        .getattr(intern!(py, "_loaded_plugins"))?;
    if !loaded_plugins.downcast::<PyDict>()?.contains(plugin)? {
        cold_path();
        let error_msg = format!("Attempted to unload a plugin '{plugin}' that is not loaded.");
        return Err(PluginUnloadError::new_err(error_msg));
    }

    try_unload_plugin(py, plugin).tap_err(|e| {
        log_exception(py, e);
    })
}

fn try_reload_plugin(py: Python, plugin: &str) -> PyResult<()> {
    if let Some(loaded_modules) = MODULES.load().as_ref() {
        if loaded_modules.bind(py).contains(plugin)? {
            loaded_modules
                .bind(py)
                .get_item(plugin)
                .and_then(|loaded_plugin_module| {
                    py.import(intern!(py, "importlib"))
                        .and_then(|importlib_module| {
                            importlib_module
                                .call_method1(intern!(py, "reload"), (loaded_plugin_module,))
                        })
                        .and_then(|module| loaded_modules.bind(py).set_item(plugin, module))
                })?;
        }
    };
    load_plugin(py, plugin)
}

#[pyfunction(name = "reload_plugin")]
fn reload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let _ = unload_plugin(py, plugin);

    try_reload_plugin(py, plugin).tap_err(|e| {
        log_exception(py, e);
    })
}

#[cfg(test)]
mod pyshinqlx_plugins_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;
    use std::{
        fs::{DirBuilder, File},
        io::Write,
        path::PathBuf,
    };

    use pyo3::{exceptions::PyEnvironmentError, intern, types::PyDict};
    use rstest::*;

    use super::{
        DEFAULT_PLUGINS, EVENT_DISPATCHERS, PluginLoadError, PluginUnloadError, load_plugin,
        load_preset_plugins, prelude::*, reload_plugin, unload_plugin,
    };
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    static TEMP_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[fixture]
    fn plugins_dir() -> String {
        let mut plugins_dir_path = PathBuf::from(TEMP_DIR.as_ref());
        plugins_dir_path.push("shinqlx-plugins");

        if !plugins_dir_path.exists() {
            DirBuilder::new()
                .create(plugins_dir_path.as_path())
                .expect("this should not happen");
        }
        let plugins_dir = plugins_dir_path.to_string_lossy().to_string();

        let mut noclass_plugin_path = PathBuf::from(&plugins_dir);
        noclass_plugin_path.push("noclass_plugin");
        noclass_plugin_path.set_extension("py");

        if !noclass_plugin_path.exists() {
            File::create(noclass_plugin_path).expect("this should not happen");
        }

        let mut nosubclass_plugin_path = PathBuf::from(&plugins_dir);
        nosubclass_plugin_path.push("nosubclass_plugin");
        nosubclass_plugin_path.set_extension("py");

        if !nosubclass_plugin_path.exists() {
            let mut test_plugin =
                File::create(nosubclass_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
class nosubclass_plugin:
    pass
        "#,
                )
                .expect("this should not happen");
        }

        let mut test_plugin_path = PathBuf::from(&plugins_dir);
        test_plugin_path.push("test_plugin");
        test_plugin_path.set_extension("py");

        if !test_plugin_path.exists() {
            let mut test_plugin = File::create(test_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
import shinqlx

class test_plugin(shinqlx.Plugin):
    pass
        "#,
                )
                .expect("this should not happen");
        }

        for plugin in DEFAULT_PLUGINS {
            let mut test_plugin_path = PathBuf::from(&plugins_dir);
            test_plugin_path.push(plugin);
            test_plugin_path.set_extension("py");

            if !test_plugin_path.exists() {
                let mut test_plugin =
                    File::create(test_plugin_path).expect("this should not happen");
                let plugin_code = format!(
                    r#"
import shinqlx

class {plugin}(shinqlx.Plugin):
    pass
                "#
                );
                test_plugin
                    .write_all(plugin_code.as_bytes())
                    .expect("this should not happen");
            }
        }

        let mut test_cmd_hook_plugin_path = PathBuf::from(&plugins_dir);
        test_cmd_hook_plugin_path.push("test_cmd_hook_plugin");
        test_cmd_hook_plugin_path.set_extension("py");

        if !test_cmd_hook_plugin_path.exists() {
            let mut test_plugin =
                File::create(test_cmd_hook_plugin_path).expect("this should not happen");
            test_plugin
                .write_all(
                    br#"
import shinqlx

class test_cmd_hook_plugin(shinqlx.Plugin):
    def __init__(self):
        self.add_command("asdf", self.cmd_asdf)

        self.add_hook("game_end", self.handle_game_end)

    def cmd_asdf(self, *args, **kwargs):
        pass

    def handle_game_end(self, *args, **kwargs):
        pass
        "#,
                )
                .expect("this should not happen");
        }

        plugins_dir_path.to_string_lossy().to_string()
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_with_main_engine_missing(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = load_preset_plugins(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugin_with_misconfigured_plugin_path_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let result = load_preset_plugins(py);
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_valid_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_default_plugins(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"DEFAULT, test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_preset_plugins_loads_unique_plugins(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let plugins_cvar_str = c"test_plugin, ban, DEFAULT, test_plugin";
        let mut raw_plugins_cvar = CVarBuilder::default()
            .string(plugins_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_plugins",
                move |_| CVar::try_from(raw_plugins_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_preset_plugins(py);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_main_engine_missing(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = load_plugin(py, "not_existing_plugin");
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_misconfigured_plugin_path_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 0..)
            .run(|| {
                Python::with_gil(|py| {
                    let result = load_plugin(py, "not_existing_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            })
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_not_existing_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "not_existing_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_test_plugin_with_no_class(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "noclass_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_with_test_plugin_with_no_subclass_of_plugin(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "nosubclass_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginLoadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_loads_valid_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    let result = load_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn load_plugin_reloads_already_loaded_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = load_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_a_non_loaded_plugin(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = unload_plugin(py, "non_loaded_plugin");
            assert!(result.is_err_and(|err| err.is_instance_of::<PluginUnloadError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unloading_loaded_plugin_with_no_event_dispatchers(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    EVENT_DISPATCHERS.store(None);

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_that_is_not_stored_in_loaded_plugins(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    py.get_type::<Plugin>()
                        .setattr(intern!(py, "_loaded_plugins"), PyDict::new(py))
                        .expect("this should not happen");

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PluginUnloadError>(py)));
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_unloads_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = unload_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unloading_loaded_plugin_unregisters_hooks_and_commands(
        _pyshinqlx_setup: (),
        plugins_dir: String,
    ) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_cvar_str = c"1";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(zmq_cvar_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_cmd_hook_plugin").expect("this should not happen");

                    let result = unload_plugin(py, "test_cmd_hook_plugin");
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reload_plugin_reloads_already_loaded_test_plugin(_pyshinqlx_setup: (), plugins_dir: String) {
        let cvar_tempdir_str = CString::new(plugins_dir).expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<UnloadDispatcher>())
                        .expect("could not add unload dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    py.import(intern!(py, "sys"))
                        .and_then(|sys_module| {
                            let full_temp_dir = TEMP_DIR
                                .path()
                                .canonicalize()
                                .expect("this should not happen");
                            sys_module
                                .getattr(intern!(py, "path"))
                                .and_then(|sys_path_module| {
                                    sys_path_module.call_method1(
                                        intern!(py, "append"),
                                        (full_temp_dir.to_string_lossy(),),
                                    )
                                })
                        })
                        .expect("this should not happen");

                    load_plugin(py, "test_plugin").expect("this should not happen");

                    let result = reload_plugin(py, "test_plugin");
                    assert!(result.is_ok());
                });
            });
    }
}

#[pyfunction(name = "initialize_cvars")]
fn initialize_cvars(py: Python<'_>) -> PyResult<()> {
    pyshinqlx_set_cvar_once(py, "qlx_owner", PyString::intern(py, "-1").as_any(), 0)?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_plugins",
        PyString::intern(py, &DEFAULT_PLUGINS.join(", ")).as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_pluginsPath",
        PyString::intern(py, "shinqlx-plugins").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_database",
        PyString::intern(py, "Redis").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_commandPrefix",
        PyString::intern(py, "!").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(py, "qlx_logs", PyString::intern(py, "2").as_any(), 0)?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_logsSize",
        PyString::intern(py, "3000000").as_any(),
        0,
    )?;

    pyshinqlx_set_cvar_once(
        py,
        "qlx_redisAddress",
        PyString::intern(py, "127.0.0.1").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_redisDatabase",
        PyString::intern(py, "0").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_redisUnixSocket",
        PyString::intern(py, "0").as_any(),
        0,
    )?;
    pyshinqlx_set_cvar_once(
        py,
        "qlx_redisPassword",
        PyString::intern(py, "").as_any(),
        0,
    )
    .map(|_| ())
}

fn register_handlers_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let sched_module = m.py().import(intern!(m.py(), "sched"))?;
    m.add(
        intern!(m.py(), "frame_tasks"),
        sched_module.call_method0(intern!(m.py(), "scheduler"))?,
    )?;
    let queue_module = m.py().import(intern!(m.py(), "queue"))?;
    m.add(
        intern!(m.py(), "next_frame_tasks"),
        queue_module.call_method0(intern!(m.py(), "SimpleQueue"))?,
    )?;

    m.add_function(wrap_pyfunction!(handlers::handle_rcon, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_client_command, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_frame, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_new_game, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_connect, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_loaded, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_disconnect, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_kamikaze_use, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_kamikaze_explode, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_damage, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_console_print, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::redirect_print, m)?)?;
    m.add_class::<handlers::PrintRedirector>()?;
    m.add_function(wrap_pyfunction!(handlers::register_handlers, m)?)?;

    Ok(())
}

#[cfg(test)]
mod initialize_cvars_tests {
    use mockall::predicate;
    use rstest::rstest;

    use super::{initialize_cvars, prelude::*};
    use crate::prelude::*;

    #[rstest]
    #[case("qlx_owner", "-1")]
    #[case(
        "qlx_plugins",
        "plugin_manager, essentials, motd, permission, ban, silence, clan, names, log, workshop"
    )]
    #[case("qlx_pluginsPath", "shinqlx-plugins")]
    #[case("qlx_database", "Redis")]
    #[case("qlx_commandPrefix", "!")]
    #[case("qlx_logs", "2")]
    #[case("qlx_logsSize", "3000000")]
    #[case("qlx_redisAddress", "127.0.0.1")]
    #[case("qlx_redisDatabase", "0")]
    #[case("qlx_redisUnixSocket", "0")]
    #[case("qlx_redisPassword", "")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_cvars_initializes_cvar_if_not_set(
        #[case] cvar_name: &'static str,
        #[case] cvar_value: &'static str,
        _pyshinqlx_setup: (),
    ) {
        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 1..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq(cvar_name),
                        predicate::eq(cvar_value),
                        predicate::eq(Some(0)),
                    )
                    .times(1);
                mock_engine.expect_get_cvar().with(
                    predicate::ne(cvar_name),
                    predicate::always(),
                    predicate::always(),
                );
            })
            .run(|| {
                Python::with_gil(|py| {
                    let result = initialize_cvars(py);
                    assert!(result.is_ok());
                });
            });
    }
}

#[pyfunction(name = "initialize")]
fn initialize(py: Python<'_>) {
    py.allow_threads(register_handlers)
}

fn try_get_plugins_path() -> Result<PathBuf, &'static str> {
    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err("no main engine found")
        },
        |main_engine| {
            main_engine.find_cvar("qlx_pluginsPath").map_or(
                {
                    cold_path();
                    Err("qlx_pluginsPath cvar not found")
                },
                |plugins_path_cvar| {
                    let plugins_path_str = plugins_path_cvar.get_string();

                    let plugins_path = Path::new(&plugins_path_str);
                    if !plugins_path.is_dir() {
                        cold_path();
                        return Err("qlx_pluginsPath is not pointing to an existing directory");
                    }

                    plugins_path
                        .canonicalize()
                        .map_err(|_| "canonicalize returned an error")
                },
            )
        },
    )
}

#[cfg(test)]
mod try_get_plugins_path_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use super::try_get_plugins_path;
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    static TEMP_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn try_get_plugins_path_with_no_main_engine() {
        let result = try_get_plugins_path();
        assert!(result.is_err_and(|err| err == "no main engine found"));
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn try_get_plugins_path_with_no_cvar_set() {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_pluginsPath", |_| None, 1..)
            .run(|| {
                let result = try_get_plugins_path();
                assert!(result.is_err_and(|err| err == "qlx_pluginsPath cvar not found"));
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn try_get_plugins_path_with_cvar_pointing_to_non_existent_directory() {
        let cvar_tempdir_str = c"non-existing-directory";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                let result = try_get_plugins_path();
                assert!(result.is_err_and(
                    |err| err == "qlx_pluginsPath is not pointing to an existing directory"
                ));
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn try_get_plugins_path_with_cvar_pointing_to_existing_directory() {
        let cvar_tempdir_str = CString::new(TEMP_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_tempdir_str.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_pluginsPath",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                let result = try_get_plugins_path();
                assert!(result.is_ok_and(|path| {
                    path == TEMP_DIR
                        .as_ref()
                        .canonicalize()
                        .expect("this should not happen")
                }));
            });
    }
}

/// Initialization that needs to be called after QLDS has finished
/// its own initialization.
#[pyfunction(name = "late_init", pass_module)]
fn late_init(module: &Bound<'_, PyModule>) -> PyResult<()> {
    initialize_cvars(module.py())?;

    MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err("no main engine found"))
        },
        |main_engine| {
            let database_cvar = main_engine.find_cvar("qlx_database");
            if database_cvar.is_some_and(|value| value.get_string().to_lowercase() == "redis") {
                let redis_database_class = module.py().get_type::<Redis>();
                module
                    .py()
                    .get_type::<Plugin>()
                    .setattr(intern!(module.py(), "database"), &redis_database_class)?;
            }

            let sys_module = module.py().import(intern!(module.py(), "sys"))?;

            if let Ok(real_plugins_path) = try_get_plugins_path() {
                set_plugins_version(module, &real_plugins_path.to_string_lossy());

                real_plugins_path
                    .parent()
                    .map(|value| value.to_string_lossy())
                    .map_or(
                        {
                            cold_path();
                            Err(PyEnvironmentError::new_err(
                                "could not determine directory name of qlx_pluginsPath",
                            ))
                        },
                        |plugins_path_dirname| {
                            let sys_path_module =
                                sys_module.getattr(intern!(module.py(), "path"))?;
                            sys_path_module.call_method1(
                                intern!(module.py(), "append"),
                                (plugins_path_dirname,),
                            )
                        },
                    )?;
            }

            pyshinqlx_configure_logger(module.py())?;
            let logger = pyshinqlx_get_logger(module.py(), None)?;

            let info_level = module
                .py()
                .import(intern!(module.py(), "logging"))
                .and_then(|logging_module| logging_module.getattr(intern!(module.py(), "INFO")))?;

            let handle_exception = module.getattr(intern!(module.py(), "handle_exception"))?;
            sys_module.setattr(intern!(module.py(), "excepthook"), handle_exception)?;

            let threading_module = module.py().import(intern!(module.py(), "threading"))?;
            let threading_except_hook =
                module.getattr(intern!(module.py(), "threading_excepthook"))?;
            threading_module.setattr(intern!(module.py(), "excepthook"), threading_except_hook)?;

            logger
                .call_method(
                    intern!(module.py(), "makeRecord"),
                    (
                        intern!(module.py(), "shinqlx"),
                        &info_level,
                        intern!(module.py(), ""),
                        -1,
                        intern!(module.py(), "Loading preset plugins..."),
                        module.py().None(),
                        module.py().None(),
                    ),
                    Some(
                        &[(
                            intern!(module.py(), "func"),
                            intern!(module.py(), "late_init"),
                        )]
                        .into_py_dict(module.py())?,
                    ),
                )
                .and_then(|log_record| {
                    logger.call_method1(intern!(module.py(), "handle"), (log_record,))
                })?;

            load_preset_plugins(module.py())?;

            let stats_enable_cvar = main_engine.find_cvar("zmq_stats_enable");
            if stats_enable_cvar.is_some_and(|value| value.get_string() != "0") {
                let stats_value = Bound::new(module.py(), StatsListener::py_new()?)?;
                module.setattr(intern!(module.py(), "_stats"), &stats_value)?;

                logger
                    .call_method(
                        intern!(module.py(), "makeRecord"),
                        (
                            intern!(module.py(), "shinqlx"),
                            &info_level,
                            intern!(module.py(), ""),
                            -1,
                            intern!(module.py(), "Stats listener started on %s."),
                            (&stats_value.get().address,),
                            module.py().None(),
                        ),
                        Some(
                            &[(
                                intern!(module.py(), "func"),
                                intern!(module.py(), "late_init"),
                            )]
                            .into_py_dict(module.py())?,
                        ),
                    )
                    .and_then(|log_record| {
                        logger.call_method1(intern!(module.py(), "handle"), (log_record,))
                    })?;

                stats_value.keep_receiving()?;
            }

            logger
                .call_method(
                    intern!(module.py(), "makeRecord"),
                    (
                        intern!(module.py(), "shinqlx"),
                        &info_level,
                        intern!(module.py(), ""),
                        -1,
                        intern!(module.py(), "We're good to go!"),
                        module.py().None(),
                        module.py().None(),
                    ),
                    Some(
                        &[(
                            intern!(module.py(), "func"),
                            intern!(module.py(), "late_init"),
                        )]
                        .into_py_dict(module.py())?,
                    ),
                )
                .and_then(|log_record| {
                    logger.call_method1(intern!(module.py(), "handle"), (log_record,))
                })
                .map(|_| ())
        },
    )
}

#[pymodule()]
#[pyo3(name = "shinqlx", gil_used = true)]
fn pyshinqlx_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let shinqlx_version = env!("SHINQLX_VERSION");
    m.add("__version__", shinqlx_version)?;

    let version_info = match semver::Version::parse(shinqlx_version) {
        Err(_) => {
            cold_path();
            (999, 999, 999)
        }
        Ok(version) => (version.major, version.minor, version.patch),
    };
    m.add("__version_info__", version_info)?;
    m.add("__plugins_version__", "NOT_SET")?;

    m.add("_map_title", "")?;
    m.add("_map_subtitle1", "")?;
    m.add("_map_subtitle2", "")?;
    m.add("DEBUG", cfg!(debug_assertions))?;

    register_shinqlx_module(m)?;
    register_core_module(m)?;
    register_game_module(m)?;
    register_player_module(m)?;
    register_commands_module(m)?;
    register_handlers_module(m)?;
    register_events_module(m)?;
    register_zmq_module(m)?;
    register_plugin_module(m)?;
    register_database_submodule(m)?;

    Ok(())
}

fn register_shinqlx_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Set a bunch of constants. We set them here because if you define functions in Python that use module
    // constants as keyword defaults, we have to always make sure they're exported first, and fuck that.
    m.add("RET_NONE", PythonReturnCodes::RET_NONE as i32)?;
    m.add("RET_STOP", PythonReturnCodes::RET_STOP as i32)?;
    m.add("RET_STOP_EVENT", PythonReturnCodes::RET_STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", PythonReturnCodes::RET_STOP_ALL as i32)?;
    m.add("RET_USAGE", PythonReturnCodes::RET_USAGE as i32)?;
    m.add("PRI_HIGHEST", CommandPriorities::PRI_HIGHEST as i32)?;
    m.add("PRI_HIGH", CommandPriorities::PRI_HIGH as i32)?;
    m.add("PRI_NORMAL", CommandPriorities::PRI_NORMAL as i32)?;
    m.add("PRI_LOW", CommandPriorities::PRI_LOW as i32)?;
    m.add("PRI_LOWEST", CommandPriorities::PRI_LOWEST as i32)?;

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

    m.add_class::<Vector3>()?;
    m.add_class::<Flight>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;

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

    Ok(())
}

fn register_core_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("PluginLoadError", m.py().get_type::<PluginLoadError>())?;
    m.add("PluginUnloadError", m.py().get_type::<PluginUnloadError>())?;

    m.add(
        "TEAMS",
        [&Teams::Free, &Teams::Red, &Teams::Blue, &Teams::Spectator]
            .map(|team| (*team as i32, team.to_string()))
            .into_py_dict(m.py())?,
    )?;
    // Game types
    m.add(
        "GAMETYPES",
        [
            &GameTypes::FreeForAll,
            &GameTypes::Duel,
            &GameTypes::Race,
            &GameTypes::TeamDeathmatch,
            &GameTypes::ClanArena,
            &GameTypes::CaptureTheFlag,
            &GameTypes::OneFlag,
            &GameTypes::Harvester,
            &GameTypes::FreezeTag,
            &GameTypes::Domination,
            &GameTypes::AttackAndDefend,
            &GameTypes::RedRover,
        ]
        .map(|gametype| (*gametype as i32, gametype.type_long()))
        .into_py_dict(m.py())?,
    )?;
    m.add(
        "GAMETYPES_SHORT",
        [
            &GameTypes::FreeForAll,
            &GameTypes::Duel,
            &GameTypes::Race,
            &GameTypes::TeamDeathmatch,
            &GameTypes::ClanArena,
            &GameTypes::CaptureTheFlag,
            &GameTypes::OneFlag,
            &GameTypes::Harvester,
            &GameTypes::FreezeTag,
            &GameTypes::Domination,
            &GameTypes::AttackAndDefend,
            &GameTypes::RedRover,
        ]
        .map(|gametype| (*gametype as i32, gametype.type_short()))
        .into_py_dict(m.py())?,
    )?;
    m.add(
        "CONNECTION_STATES",
        [
            &ConnectionStates::Free,
            &ConnectionStates::Zombie,
            &ConnectionStates::Connected,
            &ConnectionStates::Primed,
            &ConnectionStates::Active,
        ]
        .map(|connection_state| (*connection_state as i32, connection_state.to_string()))
        .into_py_dict(m.py())?,
    )?;
    // Weapons
    m.add(
        "WEAPONS",
        [
            &Weapon::Gauntlet,
            &Weapon::Machinegun,
            &Weapon::Shotgun,
            &Weapon::GrenadeLauncher,
            &Weapon::RocketLauncher,
            &Weapon::Lightning,
            &Weapon::Railgun,
            &Weapon::PlasmaGun,
            &Weapon::Bfg,
            &Weapon::GrapplingHook,
            &Weapon::NailGun,
            &Weapon::ProximityMainLauncher,
            &Weapon::ChainGun,
            &Weapon::HeavyMachinegun,
            &Weapon::Hands,
        ]
        .map(|weapon| (*weapon as i32, weapon.to_string()))
        .into_py_dict(m.py())?,
    )?;
    m.add("DEFAULT_PLUGINS", PyTuple::new(m.py(), DEFAULT_PLUGINS)?)?;

    m.add("_thread_count", 0)?;
    m.add("_thread_name", SHINQLX_THREADNAME)?;

    m.add("_stats", m.py().None())?;
    MODULES.store(Some(PyDict::new(m.py()).unbind().into()));
    m.add(
        "_modules",
        MODULES
            .load()
            .as_ref()
            .map(|modules| modules.as_ref().bind(m.py())),
    )?;

    m.add_function(wrap_pyfunction!(pyshinqlx_parse_variables, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_logger, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_configure_logger, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_log_exception, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_handle_exception, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_handle_threading_exception, m)?)?;
    m.add_function(wrap_pyfunction!(uptime, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_owner, m)?)?;
    m.add_function(wrap_pyfunction!(get_stats_listener, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_once, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_limit_once, m)?)?;
    m.add_function(wrap_pyfunction!(set_plugins_version, m)?)?;
    m.add_function(wrap_pyfunction!(set_map_subtitles, m)?)?;
    m.add_function(wrap_pyfunction!(next_frame, m)?)?;
    m.add_function(wrap_pyfunction!(delay, m)?)?;
    m.add_function(wrap_pyfunction!(thread, m)?)?;
    m.add_function(wrap_pyfunction!(load_preset_plugins, m)?)?;
    m.add_function(wrap_pyfunction!(load_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(unload_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(reload_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(initialize_cvars, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    m.add_function(wrap_pyfunction!(late_init, m)?)?;

    Ok(())
}

fn register_game_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Game>()?;
    m.add(
        "NonexistentGameError",
        m.py().get_type::<NonexistentGameError>(),
    )?;

    Ok(())
}

fn register_player_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Player>()?;
    m.add(
        "NonexistentPlayerError",
        m.py().get_type::<NonexistentPlayerError>(),
    )?;
    m.add_class::<AbstractDummyPlayer>()?;
    m.add_class::<RconDummyPlayer>()?;

    Ok(())
}

fn register_commands_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("MAX_MSG_LENGTH", MAX_MSG_LENGTH)?;
    let regex_module = m.py().import(intern!(m.py(), "re"))?;
    m.add(
        intern!(m.py(), "re_color_tag"),
        regex_module.call_method1(intern!(m.py(), "compile"), (r"\^[0-7]",))?,
    )?;
    m.add_class::<AbstractChannel>()?;
    m.add_class::<ChatChannel>()?;
    m.add_class::<TeamChatChannel>()?;
    m.add_class::<TellChannel>()?;
    m.add_class::<ConsoleChannel>()?;
    m.add_class::<ClientCommandChannel>()?;

    let all_chat_channel = Bound::new(
        m.py(),
        TeamChatChannel::py_new(
            m.py(),
            "all",
            "chat",
            "print \"{}\n\"\n",
            m.py().None().bind(m.py()),
            None,
        ),
    )?;
    CHAT_CHANNEL.store(Some(all_chat_channel.unbind().into()));
    m.add(
        "CHAT_CHANNEL",
        CHAT_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    let red_team_chat_channel = Bound::new(
        m.py(),
        TeamChatChannel::py_new(
            m.py(),
            "red",
            "red_team_chat",
            "print \"{}\n\"\n",
            m.py().None().bind(m.py()),
            None,
        ),
    )?;
    RED_TEAM_CHAT_CHANNEL.store(Some(red_team_chat_channel.unbind().into()));
    m.add(
        "RED_TEAM_CHAT_CHANNEL",
        RED_TEAM_CHAT_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    let blue_team_chat_channel = Bound::new(
        m.py(),
        TeamChatChannel::py_new(
            m.py(),
            "blue",
            "blue_team_chat",
            "print \"{}\n\"\n",
            m.py().None().bind(m.py()),
            None,
        ),
    )?;
    BLUE_TEAM_CHAT_CHANNEL.store(Some(blue_team_chat_channel.unbind().into()));
    m.add(
        "BLUE_TEAM_CHAT_CHANNEL",
        BLUE_TEAM_CHAT_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    let free_chat_channel = Bound::new(
        m.py(),
        TeamChatChannel::py_new(
            m.py(),
            "free",
            "free_chat",
            "print \"{}\n\"\n",
            m.py().None().bind(m.py()),
            None,
        ),
    )?;
    FREE_CHAT_CHANNEL.store(Some(free_chat_channel.unbind().into()));
    m.add(
        "FREE_CHAT_CHANNEL",
        FREE_CHAT_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    let spec_chat_channel = Bound::new(
        m.py(),
        TeamChatChannel::py_new(
            m.py(),
            "spectator",
            "spectator_chat",
            "print \"{}\n\"\n",
            m.py().None().bind(m.py()),
            None,
        ),
    )?;
    SPECTATOR_CHAT_CHANNEL.store(Some(spec_chat_channel.unbind().into()));
    m.add(
        "SPECTATOR_CHAT_CHANNEL",
        SPECTATOR_CHAT_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    CONSOLE_CHANNEL.store(Some(
        Py::new(
            m.py(),
            ConsoleChannel::py_new(m.py(), m.py().None().bind(m.py()), None),
        )?
        .into(),
    ));
    m.add(
        "CONSOLE_CHANNEL",
        CONSOLE_CHANNEL
            .load()
            .as_ref()
            .map(|channel| channel.as_ref().bind(m.py())),
    )?;
    m.add_class::<Command>()?;
    m.add_class::<CommandInvoker>()?;
    COMMANDS.store(Some(Py::new(m.py(), CommandInvoker::py_new())?.into()));
    m.add(
        "COMMANDS",
        COMMANDS
            .load()
            .as_ref()
            .map(|commands| commands.as_ref().bind(m.py())),
    )?;

    Ok(())
}

fn register_events_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let regex_module = m.py().import(intern!(m.py(), "re"))?;
    m.add(
        intern!(m.py(), "_re_vote"),
        regex_module.call_method1(
            intern!(m.py(), "compile"),
            (r#"^(?P<cmd>[^ ]+)(?: "?(?P<args>.*?)"?)?$"#,),
        )?,
    )?;
    m.add_class::<EventDispatcher>()?;
    m.add_class::<ConsolePrintDispatcher>()?;
    m.add_class::<CommandDispatcher>()?;
    m.add_class::<ClientCommandDispatcher>()?;
    m.add_class::<ServerCommandDispatcher>()?;
    m.add_class::<FrameEventDispatcher>()?;
    m.add_class::<SetConfigstringDispatcher>()?;
    m.add_class::<ChatEventDispatcher>()?;
    m.add_class::<UnloadDispatcher>()?;
    m.add_class::<PlayerConnectDispatcher>()?;
    m.add_class::<PlayerLoadedDispatcher>()?;
    m.add_class::<PlayerDisconnectDispatcher>()?;
    m.add_class::<PlayerSpawnDispatcher>()?;
    m.add_class::<StatsDispatcher>()?;
    m.add_class::<VoteCalledDispatcher>()?;
    m.add_class::<VoteStartedDispatcher>()?;
    m.add_class::<VoteEndedDispatcher>()?;
    m.add_class::<VoteDispatcher>()?;
    m.add_class::<GameCountdownDispatcher>()?;
    m.add_class::<GameStartDispatcher>()?;
    m.add_class::<GameEndDispatcher>()?;
    m.add_class::<RoundCountdownDispatcher>()?;
    m.add_class::<RoundStartDispatcher>()?;
    m.add_class::<RoundEndDispatcher>()?;
    m.add_class::<TeamSwitchDispatcher>()?;
    m.add_class::<TeamSwitchAttemptDispatcher>()?;
    m.add_class::<MapDispatcher>()?;
    m.add_class::<NewGameDispatcher>()?;
    m.add_class::<KillDispatcher>()?;
    m.add_class::<DeathDispatcher>()?;
    m.add_class::<UserinfoDispatcher>()?;
    m.add_class::<KamikazeUseDispatcher>()?;
    m.add_class::<KamikazeExplodeDispatcher>()?;
    m.add_class::<DamageDispatcher>()?;
    m.add_class::<EventDispatcherManager>()?;

    let event_dispatchers = Bound::new(m.py(), EventDispatcherManager::default())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<ConsolePrintDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<CommandDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<ClientCommandDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<ServerCommandDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<FrameEventDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<SetConfigstringDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<ChatEventDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<UnloadDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<PlayerConnectDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<PlayerLoadedDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<PlayerDisconnectDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<PlayerSpawnDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<KamikazeUseDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<KamikazeExplodeDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<StatsDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<VoteCalledDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<VoteStartedDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<VoteEndedDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<VoteDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<GameCountdownDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<GameStartDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<GameEndDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<RoundCountdownDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<RoundStartDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<RoundEndDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<TeamSwitchDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<TeamSwitchAttemptDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<MapDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<NewGameDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<KillDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<DeathDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<UserinfoDispatcher>())?;
    event_dispatchers.add_dispatcher(&m.py().get_type::<DamageDispatcher>())?;
    EVENT_DISPATCHERS.store(Some(event_dispatchers.unbind().into()));
    m.add(
        "EVENT_DISPATCHERS",
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .map(|event_dispatchers| event_dispatchers.as_ref().bind(m.py())),
    )?;

    Ok(())
}

fn register_zmq_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<StatsListener>()?;

    Ok(())
}

fn register_plugin_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.py()
        .get_type::<Plugin>()
        .setattr(intern!(m.py(), "database"), m.py().None())?;
    m.py()
        .get_type::<Plugin>()
        .setattr(intern!(m.py(), "_loaded_plugins"), PyDict::new(m.py()))?;
    m.add_class::<Plugin>()?;

    Ok(())
}

fn register_database_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let database_module = PyModule::new(m.py(), "database")?;

    database_module.add_class::<AbstractDatabase>()?;
    database_module.add_class::<Redis>()?;
    m.add_submodule(&database_module)?;

    m.py()
        .import(intern!(m.py(), "sys"))?
        .getattr(intern!(m.py(), "modules"))?
        .set_item("shinqlx.database", database_module)?;

    Ok(())
}

pub(crate) static PYSHINQLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn pyshinqlx_is_initialized() -> bool {
    PYSHINQLX_INITIALIZED.load(Ordering::Acquire)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum PythonInitializationError {
    MainScriptError,
    AlreadyInitialized,
    NotInitializedError,
}

pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
    if pyshinqlx_is_initialized() {
        cold_path();
        error!(target: "shinqlx", "pyshinqlx_initialize was called while already initialized");
        return Err(PythonInitializationError::AlreadyInitialized);
    }

    debug!(target: "shinqlx", "Initializing Python...");
    if unsafe { Py_IsInitialized() } == 0 {
        append_to_inittab!(pyshinqlx_module);
    }
    prepare_freethreaded_python();
    Python::with_gil(|py| {
        py.import(intern!(py, "shinqlx")).and_then(|shinqlx_module| {
            shinqlx_module.call_method0(intern!(py, "initialize"))
        }).map_or_else(|err| {
            cold_path();
            error!(target: "shinqlx", "{err:?}");
            error!(target: "shinqlx", "loader sequence returned an error. Did you modify the loader?");
            Err(PythonInitializationError::MainScriptError)
        }, |_| {
                PYSHINQLX_INITIALIZED.store(true, Ordering::Release);
                debug!(target: "shinqlx", "Python initialized!");
                Ok(())
            })
    })
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        error!(target: "shinqlx", "pyshinqlx_finalize was called before being initialized");
        return Err(PythonInitializationError::NotInitializedError);
    }

    CUSTOM_COMMAND_HANDLER.store(None);

    Python::with_gil(|py| {
        py.import(intern!(py, "importlib"))
            .and_then(|importlib_module| {
                let shinqlx_module = py.import(intern!(py, "shinqlx"))?;
                importlib_module
                    .call_method1(intern!(py, "reload"), (shinqlx_module,))?
                    .call_method0(intern!(py, "initialize"))
            })
            .map_or_else(
                |_| {
                    cold_path();
                    PYSHINQLX_INITIALIZED.store(false, Ordering::Release);
                    Err(PythonInitializationError::MainScriptError)
                },
                |_| {
                    PYSHINQLX_INITIALIZED.store(true, Ordering::Release);
                    Ok(())
                },
            )
    })
}

#[cfg(test)]
mod pyshinqlx_tests {
    use core::sync::atomic::Ordering;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{
        PYSHINQLX_INITIALIZED, PythonInitializationError, prelude::*, pyshinqlx_initialize,
        pyshinqlx_reload,
    };
    use crate::prelude::*;

    #[test]
    #[serial]
    fn initialize_when_python_already_initialized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::Release);

        let result = pyshinqlx_initialize();

        assert!(result.is_err_and(|err| err == PythonInitializationError::AlreadyInitialized));
        assert_eq!(PYSHINQLX_INITIALIZED.load(Ordering::Acquire), true);
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn initialize_when_python_init_succeeds(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(false, Ordering::Release);

        let _shinqlx_module = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                cr#"""
def initialize():
    pass
"""#,
                c"shinqlx.py",
                c"shinqlx",
            )
            .expect("this should not happen")
            .unbind()
        });

        let result = pyshinqlx_initialize();

        assert!(result.is_ok());
        assert_eq!(PYSHINQLX_INITIALIZED.load(Ordering::Acquire), true);
    }

    #[test]
    #[serial]
    fn reload_when_python_not_initialized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::Release);

        let result = pyshinqlx_reload();

        assert!(result.is_err_and(|err| err == PythonInitializationError::NotInitializedError));
        assert_eq!(PYSHINQLX_INITIALIZED.load(Ordering::Acquire), false);
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
pub(crate) mod pyshinqlx_test_support {
    use alloc::ffi::CString;
    use core::fmt::Debug;

    use pyo3::{intern, prelude::*};

    use super::{
        Command, Flight, Holdable, Player, PlayerInfo, PlayerState, Powerups, Vector3, Weapons,
    };
    use crate::ffi::c::prelude::{clientState_t, privileges_t, team_t, weapon_t};

    pub(crate) fn run_all_frame_tasks(py: Python<'_>) -> PyResult<()> {
        py.run(
            cr#"
import shinqlx

while not shinqlx.next_frame_tasks.empty():
    func, args, kwargs = shinqlx.next_frame_tasks.get(block=False)
    func(*args, **kwargs)
"#,
            None,
            None,
        )?;
        Ok(())
    }

    pub(crate) fn default_test_player_info() -> PlayerInfo {
        PlayerInfo {
            client_id: 2,
            name: "".to_string(),
            connection_state: clientState_t::CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        }
    }

    pub(crate) fn default_test_player() -> Player {
        Player {
            valid: true.into(),
            id: 2,
            player_info: default_test_player_info().into(),
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "".to_string().into(),
        }
    }

    pub(crate) fn default_player_state() -> PlayerState {
        PlayerState {
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
            holdable: Holdable::Kamikaze,
            flight: Flight(12, 34, 56, 78),
            is_chatting: true,
            is_frozen: true,
        }
    }

    pub(crate) fn default_command(py: Python<'_>) -> Command {
        let capturing_hook = capturing_hook(py);
        Command {
            plugin: test_plugin(py)
                .call0()
                .expect("this should not happen")
                .unbind(),
            name: vec!["cmd_name".into()],
            handler: capturing_hook
                .getattr(intern!(py, "hook"))
                .expect("could not get capturing hook")
                .unbind(),
            permission: 0,
            channels: vec![].into(),
            exclude_channels: vec![].into(),
            client_cmd_pass: false,
            client_cmd_perm: 0,
            prefix: true,
            usage: "".to_string(),
        }
    }

    pub(super) fn test_plugin(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            cr#"
import shinqlx

class test_plugin(shinqlx.Plugin):
    pass
"#,
            c"",
            c"",
        )
        .expect("coult not create test plugin")
        .getattr(intern!(py, "test_plugin"))
        .expect("could not get test plugin")
    }

    pub(crate) fn capturing_hook(py: Python<'_>) -> Bound<'_, PyModule> {
        PyModule::from_code(
            py,
            cr#"
_args = []

def hook(*args):
    global _args
    _args.append(args)

def assert_called_with(*args):
    global _args
    assert(len(_args) > 0), f"{_args = }"

    called_with = _args.pop(0)
    assert len(args) == len(called_with), f"{args = } {len(args) = } == {called_with = } {len(called_with) = }"
    for (expected, actual) in zip(args, called_with):
        if expected == "_":
            continue
        assert expected == actual, f"{expected = } == {actual = }"
        "#,
            c"",
            c"",
        )
            .expect("could create test handler module")
    }

    pub(crate) fn python_function_returning<'py, T: Debug>(
        py: Python<'py>,
        returned: &T,
    ) -> Bound<'py, PyAny> {
        let module_definition = format!(
            r#"
def custom_return(*args, **kwargs):
    return {returned:?}
            "#,
        );
        PyModule::from_code(
            py,
            &CString::new(module_definition).expect("this should not happen"),
            c"",
            c"",
        )
        .expect("could not create returning string module")
        .getattr(intern!(py, "custom_return"))
        .expect("could not get returning_string function")
    }

    pub(crate) fn python_function_raising_exception(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            cr#"
def raises_exception(*args, **kwargs):
    raise ValueError("asdf")
            "#,
            c"",
            c"",
        )
        .expect("this should not happen")
        .getattr(intern!(py, "raises_exception"))
        .expect("this should not happen")
    }
}

#[cfg(test)]
#[mockall::automock]
#[allow(dead_code)]
pub(crate) mod python_tests {
    use super::PythonInitializationError;

    #[cfg(not(tarpaulin_include))]
    pub(crate) fn rcon_dispatcher<T>(_cmd: T)
    where
        T: AsRef<str> + 'static,
    {
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn client_command_dispatcher(_client_id: i32, _cmd: String) -> Option<String> {
        None
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn server_command_dispatcher(
        _client_id: Option<i32>,
        _cmd: String,
    ) -> Option<String> {
        None
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn client_loaded_dispatcher(_client_id: i32) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: &str) -> Option<String> {
        None
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: &str) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn console_print_dispatcher(_msg: &str) -> Option<String> {
        None
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn new_game_dispatcher(_restart: bool) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn frame_dispatcher() {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn client_connect_dispatcher(_client_id: i32, _is_bot: bool) -> Option<String> {
        None
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn client_spawn_dispatcher(_client_id: i32) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn kamikaze_use_dispatcher(_client_id: i32) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn kamikaze_explode_dispatcher(_client_id: i32, _is_used_on_demand: bool) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn damage_dispatcher(
        _target_client_id: i32,
        _attacker_client_id: Option<i32>,
        _damage: i32,
        _dflags: i32,
        _means_of_death: i32,
    ) {
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn pyshinqlx_is_initialized() -> bool {
        false
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod pyshinqlx_setup_fixture {
    use pyo3::{append_to_inittab, ffi::Py_IsInitialized, prepare_freethreaded_python};
    use rstest::fixture;

    use super::pyshinqlx_module;

    #[fixture]
    #[once]
    pub(crate) fn pyshinqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyshinqlx_module);
            prepare_freethreaded_python();
        }
    }
}
