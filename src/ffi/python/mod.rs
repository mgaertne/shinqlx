#[cfg(feature = "bench-support")]
pub mod bench_support;
mod channels;
mod commands;
mod database;
mod decorators;
mod deferred;
mod dispatchers;
mod embed;
mod engine_api;
mod events;
mod flight;
mod game;
mod handlers;
mod holdable;
mod int_tuple;
mod logging;
mod module_registration;
mod native_plugins;
mod player;
mod player_info;
mod player_state;
mod player_stats;
mod plugin;
mod plugin_loader;
mod powerups;
mod stats_listener;
mod text;
mod vector3;
mod weapons;

pub(crate) mod prelude {
    pub(crate) use pyo3::prelude::*;

    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
    pub(crate) use super::channels::{
        AbstractChannel, AbstractChannelMethods, ChatChannel, ChatChannelMethods,
        ClientCommandChannel, ClientCommandChannelMethods, ConsoleChannel, MAX_MSG_LENGTH,
        TeamChatChannel, TellChannel, TellChannelMethods,
    };
    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
    pub(crate) use super::commands::{
        Command, CommandInvoker, CommandInvokerMethods, CommandMethods,
    };
    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
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
    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
    pub(crate) use super::game::{Game, GameMethods, NonexistentGameError};
    #[cfg(test)]
    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command, handle_console_print, handle_damage, handle_frame,
        handle_kamikaze_explode, handle_kamikaze_use, handle_new_game, handle_player_connect,
        handle_player_disconnect, handle_player_loaded, handle_player_spawn, handle_rcon,
        handle_server_command, handle_set_configstring, register_handlers,
    };
    #[cfg(test)]
    #[allow(
        unused_imports,
        reason = "re-exported for the prelude, not every build uses every item"
    )]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command_context, handle_console_print_context, handle_damage_context,
        handle_frame_context, handle_kamikaze_explode_context, handle_kamikaze_use_context,
        handle_new_game_context, handle_player_connect_context, handle_player_disconnect_context,
        handle_player_loaded_context, handle_player_spawn_context, handle_rcon_context,
        handle_server_command_context, handle_set_configstring_context, register_handlers_context,
    };
    #[cfg(not(test))]
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
    #[cfg(test)]
    pub(crate) use super::pyshinqlx_setup_fixture::*;
    pub(crate) use super::{
        ALLOW_FREE_CLIENT, CUSTOM_COMMAND_HANDLER, PythonInitializationError, clean_text,
        embed::*,
        events::*,
        flight::Flight,
        holdable::Holdable,
        parse_variables,
        player::{
            AbstractDummyPlayer, NonexistentPlayerError, Player, PlayerMethods, RconDummyPlayer,
        },
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
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use arc_swap::ArcSwapOption;
use chrono::Utc;
use decorators::{delay, next_frame, thread};
use derive_more::Display;
pub(crate) use engine_api::*;
use log::*;
pub(crate) use logging::{log_debug, log_exception, log_plugin_exception, pyshinqlx_get_logger};
use logging::{
    pyshinqlx_configure_logger, pyshinqlx_handle_exception, pyshinqlx_handle_threading_exception,
    pyshinqlx_log_exception,
};
use module_registration::{late_init, pyshinqlx_module};
use plugin_loader::load_preset_plugins;
pub(crate) use plugin_loader::{DEFAULT_PLUGINS, load_plugin, reload_plugin, unload_plugin};
use prelude::*;
use pyo3::{
    append_to_inittab, create_exception,
    exceptions::{PyEnvironmentError, PyException, PyValueError},
    ffi::Py_IsInitialized,
    intern,
    types::{PyBool, PyDelta, PyDict, PyString},
};
use tap::{TapOptional, TryConv};
use text::pyshinqlx_parse_variables;
pub(crate) use text::{ParsedVariables, clean_text, last_color_tag, parse_variables};

use crate::{_INIT_TIME, MAIN_ENGINE, ffi::c::prelude::*, quake_live_engine::FindCVar};

const SHINQLX_THREADNAME: &str = "shinqlxthread";

pub(crate) static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) static CUSTOM_COMMAND_HANDLER: LazyLock<ArcSwapOption<Py<PyAny>>> =
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

/// Whether the value is the Python `False`, which is how handlers cancel an event.
pub(crate) fn is_false(value: &Bound<'_, PyAny>) -> bool {
    value
        .cast::<PyBool>()
        .is_ok_and(|python_bool| !python_bool.is_true())
}

/// Returns the event dispatcher registered under the given event name, or an environment error
/// carrying `error_msg` if there is none.
pub(crate) fn event_dispatcher<'py>(
    py: Python<'py>,
    event: &Bound<'py, PyString>,
    error_msg: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| event_dispatchers.bind(py).get_item(event).ok())
        .ok_or_else(|| {
            cold_path();
            PyEnvironmentError::new_err(error_msg)
        })
}

/// Return codes of Python handlers, defined here and added to the module using `PyModule_AddIntMacro()`.
#[allow(
    non_camel_case_types,
    reason = "the variants keep the RET_* names of minqlx"
)]
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PythonReturnCodes {
    RET_NONE,
    /// Stop execution of event handlers within Python.
    RET_STOP,
    /// Only stop the event, but let other handlers process it.
    RET_STOP_EVENT,
    /// Stop execution at an engine level. SCARY STUFF!
    RET_STOP_ALL,
    /// Used for commands. Replies to the channel with a command's usage.
    RET_USAGE,
}

impl FromPyObject<'_, '_> for PythonReturnCodes {
    type Error = PyErr;

    fn extract(item: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
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
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(py.None().bind_borrowed(py))
                    .is_ok_and(|value| value == PythonReturnCodes::RET_NONE)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_bool_true(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(PyBool::new(py, true).as_any().as_borrowed())
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_bool_false(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(PyBool::new(py, false).as_any().as_borrowed())
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
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(
                    python_value
                        .into_pyobject(py)
                        .expect("this should not happen")
                        .as_any()
                        .as_borrowed()
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
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(
                    wrong_value
                        .into_pyobject(py)
                        .expect("this should not happen")
                        .as_any()
                        .as_borrowed()
                )
                .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn extract_from_str(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            assert!(
                PythonReturnCodes::extract(PyString::intern(py, "asdf").as_any().as_borrowed())
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

/// Returns a :class:`datetime.timedelta` instance of the time since initialized.
#[pyfunction]
fn uptime(py: Python<'_>) -> PyResult<Bound<'_, PyDelta>> {
    let (elapsed_days, elapsed_seconds, elapsed_microseconds) = py.detach(|| {
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

        Python::attach(|py| {
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
    py.detach(owner)
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
        Python::attach(|py| {
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
                Python::attach(|py| {
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
                Python::attach(|py| {
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
                Python::attach(|py| {
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
                Python::attach(|py| {
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
        Python::attach(|py| {
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
                Python::attach(|py| {
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
                        let returned = format!("{}-{}", plugins_version, branch_name);
                        Ok(returned)
                    })
            })
    })
}

fn get_plugins_version(path: &str) -> String {
    try_get_plugins_version(path).unwrap_or_else(|_| "NOT_SET".to_string())
}

#[pyfunction(name = "set_plugins_version", pass_module)]
fn set_plugins_version(module: &Bound<'_, PyModule>, path: &str) {
    let plugins_version = module.py().detach(|| get_plugins_version(path));

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

#[cfg(test)]
mod python_signature_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::prelude::pyshinqlx_setup;
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn all_text_signatures_are_valid_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let broken = PyModule::from_code(
                py,
                cr#"
import ast
import builtins
import inspect
import re

import shinqlx


def problem_of(text):
    text = re.sub(r"^\(\$", "(", text)
    try:
        arguments = ast.parse(f"def f{text}: pass").body[0].args
    except SyntaxError as error:
        return f"{error.msg} in {text}"
    defaults = [*arguments.defaults, *(d for d in arguments.kw_defaults if d is not None)]
    for default in defaults:
        for node in ast.walk(default):
            if isinstance(node, ast.Name) and not (
                hasattr(shinqlx, node.id) or hasattr(builtins, node.id)
            ):
                return f"unknown name {node.id} in {text}"
    return None


def broken_signatures():
    found = []
    for name in dir(shinqlx):
        obj = getattr(shinqlx, name)
        targets = [(name, obj)]
        if inspect.isclass(obj):
            targets += [
                (f"{name}.{attr}", getattr(obj, attr, None))
                for attr in dir(obj)
                if not attr.startswith("__") or attr in ("__init__", "__new__")
            ]
        for label, target in targets:
            text = getattr(target, "__text_signature__", None)
            if text is None:
                continue
            problem = problem_of(text)
            if problem is not None:
                found.append(f"{label}: {problem}")
    return found
"#,
                c"",
                c"",
            )
            .and_then(|module| module.call_method0(intern!(py, "broken_signatures")))
            .and_then(|found| found.extract::<Vec<String>>())
            .expect("could not check the signatures");

            assert!(broken.is_empty(), "invalid signatures: {broken:#?}");
        });
    }
}

#[cfg(test)]
mod python_stub_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::prelude::pyshinqlx_setup;
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn everything_the_stubs_declare_exists_at_runtime(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let missing = PyModule::from_code(
                py,
                cr#"
import ast
import pathlib

import shinqlx

TYPING_ONLY = {"NamedTuple", "Protocol", "TypedDict"}


def resolve(class_name):
    for holder in (shinqlx, shinqlx.database):
        found = getattr(holder, class_name, None)
        if found is not None:
            return found
    return None


def missing_members(stub_directory):
    found = []
    for stub in sorted(pathlib.Path(stub_directory).glob("*.pyi")):
        tree = ast.parse(stub.read_text())
        for node in tree.body:
            if isinstance(node, ast.FunctionDef):
                if not hasattr(shinqlx, node.name):
                    found.append(f"{stub.name}: function {node.name}")
            elif isinstance(node, ast.ClassDef):
                if TYPING_ONLY & {getattr(base, "id", None) for base in node.bases}:
                    continue
                runtime_class = resolve(node.name)
                if runtime_class is None:
                    found.append(f"{stub.name}: class {node.name}")
                    continue
                for member in node.body:
                    if not isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        continue
                    if member.name.startswith("__") or hasattr(runtime_class, member.name):
                        continue
                    found.append(f"{stub.name}: {node.name}.{member.name}")
    return sorted(set(found))
"#,
                c"",
                c"",
            )
            .and_then(|module| {
                module.call_method1(
                    intern!(py, "missing_members"),
                    (concat!(env!("CARGO_MANIFEST_DIR"), "/python/shinqlx"),),
                )
            })
            .and_then(|found| found.extract::<Vec<String>>())
            .expect("could not compare the stubs with the runtime");

            assert!(missing.is_empty(), "declared but missing: {missing:#?}");
        });
    }
}

#[cfg(test)]
mod python_stub_hook_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::prelude::pyshinqlx_setup;
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_hook_stubs_match_the_event_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let differences = PyModule::from_code(
                py,
                cr#"
import ast
import inspect
import pathlib

import shinqlx

POSITIONAL = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)


def literal_event(function):
    for argument in function.args.args:
        if argument.arg == "event":
            annotation = argument.annotation
            if getattr(annotation.value, "id", None) == "Literal":
                return annotation.slice.value
    return None


def parameter_count(alias):
    return len(alias.slice.elts[0].elts)


def aliased(annotation, aliases):
    return aliases[getattr(annotation, "value", annotation).id]


def stub_events(stub):
    tree = ast.parse(pathlib.Path(stub).read_text())
    aliases = {
        node.targets[0].id: node.value
        for node in tree.body
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Subscript)
    }
    plugin = next(
        node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Plugin"
    )

    families = {"add_hook": {}, "remove_hook": {}, "hook": {}}
    for function in plugin.body:
        if isinstance(function, ast.FunctionDef) and function.name in ("add_hook", "remove_hook"):
            handler = next(a for a in function.args.args if a.arg == "handler")
            families[function.name][literal_event(function)] = parameter_count(
                aliased(handler.annotation, aliases)
            )
    for function in tree.body:
        if isinstance(function, ast.FunctionDef) and function.name == "hook":
            decorator_argument = function.returns.slice.elts[0].elts[0]
            families["hook"][literal_event(function)] = (
                parameter_count(aliased(decorator_argument, aliases)) - 1
            )
    return families


def runtime_events():
    return {
        member
        for member in vars(shinqlx).values()
        if isinstance(member, type)
        and issubclass(member, shinqlx.EventDispatcher)
        and member is not shinqlx.EventDispatcher
    }


HOOK_ARITY = {"vote_started": 3, "vote_ended": 4}


def dispatcher_arity(event):
    if event in HOOK_ARITY:
        return HOOK_ARITY[event]
    parameters = inspect.signature(shinqlx.EVENT_DISPATCHERS[event].dispatch).parameters.values()
    if any(parameter.kind is inspect.Parameter.VAR_POSITIONAL for parameter in parameters):
        return None
    return len([parameter for parameter in parameters if parameter.kind in POSITIONAL])


def differences(stub):
    families = stub_events(stub)
    found = []
    declared = set(families["add_hook"])
    for family, events in families.items():
        if set(events) != declared:
            found.append(f"{family}: {sorted(set(events) ^ declared)} differs from add_hook")

    available = {
        member.name for member in runtime_events() if isinstance(getattr(member, "name", None), str)
    }
    for event in sorted(declared - available):
        found.append(f"{event}: declared in the stubs, but no dispatcher exists")
    for event in sorted(available - declared):
        found.append(f"{event}: dispatcher exists, but the stubs do not declare it")

    for event in sorted(declared & available):
        expected = dispatcher_arity(event)
        for family, events in families.items():
            if expected is not None and events.get(event) != expected:
                found.append(
                    f"{family}({event}): stub takes {events.get(event)}, dispatcher passes {expected}"
                )
    return found
"#,
                c"",
                c"",
            )
            .and_then(|module| {
                module.call_method1(
                    intern!(py, "differences"),
                    (concat!(env!("CARGO_MANIFEST_DIR"), "/python/shinqlx/_plugin.pyi"),),
                )
            })
            .and_then(|found| found.extract::<Vec<String>>())
            .expect("could not compare the hook stubs with the dispatchers");

            assert!(
                differences.is_empty(),
                "hook stubs and dispatchers differ: {differences:#?}"
            );
        });
    }
}

#[cfg(test)]
mod conversion_tests {
    use rstest::rstest;

    use super::{Teams, Weapon, parse_variables};
    use crate::ffi::c::prelude::{team_t, weapon_t};

    #[rstest]
    #[case(team_t::TEAM_FREE, Teams::Free)]
    #[case(team_t::TEAM_RED, Teams::Red)]
    #[case(team_t::TEAM_BLUE, Teams::Blue)]
    #[case(team_t::TEAM_SPECTATOR, Teams::Spectator)]
    #[case(team_t::TEAM_NUM_TEAMS, Teams::Invalid)]
    fn teams_are_converted_from_the_engine_teams(#[case] team: team_t, #[case] expected: Teams) {
        assert_eq!(Teams::from(team) as i32, expected as i32);
    }

    #[rstest]
    #[case(weapon_t::WP_GAUNTLET, Weapon::Gauntlet)]
    #[case(weapon_t::WP_MACHINEGUN, Weapon::Machinegun)]
    #[case(weapon_t::WP_SHOTGUN, Weapon::Shotgun)]
    #[case(weapon_t::WP_GRENADE_LAUNCHER, Weapon::GrenadeLauncher)]
    #[case(weapon_t::WP_ROCKET_LAUNCHER, Weapon::RocketLauncher)]
    #[case(weapon_t::WP_LIGHTNING, Weapon::Lightning)]
    #[case(weapon_t::WP_RAILGUN, Weapon::Railgun)]
    #[case(weapon_t::WP_PLASMAGUN, Weapon::PlasmaGun)]
    #[case(weapon_t::WP_BFG, Weapon::Bfg)]
    #[case(weapon_t::WP_GRAPPLING_HOOK, Weapon::GrapplingHook)]
    #[case(weapon_t::WP_NAILGUN, Weapon::NailGun)]
    #[case(weapon_t::WP_PROX_LAUNCHER, Weapon::ProximityMainLauncher)]
    #[case(weapon_t::WP_CHAINGUN, Weapon::ChainGun)]
    #[case(weapon_t::WP_HMG, Weapon::HeavyMachinegun)]
    #[case(weapon_t::WP_HANDS, Weapon::Hands)]
    #[case(weapon_t::WP_NONE, Weapon::Hands)]
    fn weapons_are_converted_from_the_engine_weapons(
        #[case] weapon: weapon_t,
        #[case] expected: Weapon,
    ) {
        assert_eq!(Weapon::from(weapon) as i32, expected as i32);
    }

    #[rstest]
    fn parsed_variables_return_the_value_of_a_known_key() {
        let variables = parse_variables("\\map\\campgrounds\\g_gametype\\4");

        assert_eq!(variables.get("map"), Some("campgrounds".to_string()));
        assert_eq!(variables.get("g_gametype"), Some("4".to_string()));
        assert_eq!(variables.get("unknown"), None);
        assert!(variables.contains("map"));
        assert!(!variables.contains("unknown"));
    }
}

#[cfg(test)]
mod callable_guard_tests {
    use pyo3::{exceptions::PyValueError, prelude::*, types::PyInt};
    use rstest::rstest;

    use super::{next_frame, prelude::pyshinqlx_setup, thread};
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_rejects_an_object_that_is_not_callable(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = next_frame(PyInt::new(py, 1).as_any());

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                    && err.value(py).to_string() == "func has to be a callable Python function")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_rejects_an_object_that_is_not_callable(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = thread(PyInt::new(py, 1).as_any());

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                    && err.value(py).to_string() == "func has to be a callable Python function")
            );
        });
    }
}

#[cfg(test)]
mod python_stub_signature_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::prelude::pyshinqlx_setup;
    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stub_signatures_match_the_runtime(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let differences = PyModule::from_code(
                py,
                cr#"
import ast
import inspect
import pathlib
import re
import types

import shinqlx

TYPING_ONLY = {"NamedTuple", "Protocol", "TypedDict"}
SELF_NAMES = {"self", "cls", "_cls", "type", "module"}


def resolve(class_name):
    for holder in (shinqlx, shinqlx.database):
        found = getattr(holder, class_name, None)
        if found is not None:
            return found
    return None


def shape(arguments, drop_first):
    positional = [a.arg for a in [*arguments.posonlyargs, *arguments.args]]
    first_default = len(positional) - len(arguments.defaults)
    positional = [(name, index >= first_default) for index, name in enumerate(positional)]
    if drop_first and positional:
        positional = positional[1:]
    return {
        "positional": positional,
        "keyword_only": {
            a.arg: d is not None for a, d in zip(arguments.kwonlyargs, arguments.kw_defaults)
        },
        "var_positional": arguments.vararg is not None,
        "var_keyword": arguments.kwarg is not None,
    }


def runtime_shape(target):
    text = getattr(target, "__text_signature__", None)
    if text is None:
        return None
    has_receiver = text.startswith("($")
    text = re.sub(r"^\(\$", "(", text)
    return shape(ast.parse(f"def f{text}: pass").body[0].args, has_receiver)


def decorator_names(node):
    names = set()
    for decorator in node.decorator_list:
        if isinstance(decorator, ast.Name):
            names.add(decorator.id)
        elif isinstance(decorator, ast.Attribute):
            names.add(decorator.attr)
    return names


def differences_of(label, stub_shape, runtime):
    if runtime is None:
        return [f"{label}: runtime has no signature"]
    found = []
    stub_positional, runtime_positional = stub_shape["positional"], runtime["positional"]
    if len(stub_positional) != len(runtime_positional):
        found.append(
            f"{label}: positional stub={[n for n, _ in stub_positional]}"
            f" runtime={[n for n, _ in runtime_positional]}"
        )
    else:
        for (stub_name, stub_default), (runtime_name, runtime_default) in zip(
            stub_positional, runtime_positional
        ):
            if not stub_name.startswith("_") and stub_name != runtime_name:
                found.append(f"{label}: parameter name stub={stub_name} runtime={runtime_name}")
            if stub_default != runtime_default:
                found.append(
                    f"{label}: parameter {stub_name or runtime_name}"
                    f" default stub={stub_default} runtime={runtime_default}"
                )
    for key in ("keyword_only", "var_positional", "var_keyword"):
        if stub_shape[key] != runtime[key]:
            found.append(f"{label}: {key} stub={stub_shape[key]} runtime={runtime[key]}")
    return found


def compare_class(stub_name, node, runtime_class):
    found = []
    members = {}
    for member in node.body:
        if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
            members.setdefault(member.name, []).append(member)
    for name, definitions in members.items():
        label = f"{stub_name}: {node.name}.{name}"
        if name.startswith("__") and name != "__init__":
            continue
        if name == "__init__":
            if len(definitions) == 1:
                found += differences_of(
                    label, shape(definitions[0].args, True), runtime_shape(runtime_class)
                )
            continue
        if not hasattr(runtime_class, name):
            continue
        static = inspect.getattr_static(runtime_class, name)
        decorators = set().union(*(decorator_names(d) for d in definitions))
        is_property = "property" in decorators
        runtime_is_property = isinstance(
            static, (property, types.GetSetDescriptorType, types.MemberDescriptorType)
        )
        if is_property != runtime_is_property:
            found.append(f"{label}: stub property={is_property} runtime property={runtime_is_property}")
            continue
        if is_property:
            continue
        runtime_is_class = isinstance(static, (classmethod, types.ClassMethodDescriptorType))
        runtime_is_static = isinstance(static, (staticmethod, types.BuiltinFunctionType))
        if ("classmethod" in decorators) != runtime_is_class:
            found.append(f"{label}: stub classmethod={'classmethod' in decorators} runtime={runtime_is_class}")
        if ("staticmethod" in decorators) != runtime_is_static:
            found.append(f"{label}: stub staticmethod={'staticmethod' in decorators} runtime={runtime_is_static}")
        if len(definitions) != 1:
            continue
        drop_first = "staticmethod" not in decorators
        found += differences_of(
            label,
            shape(definitions[0].args, drop_first),
            runtime_shape(getattr(runtime_class, name)),
        )
    return found


def differences(stub_directory):
    found = []
    for stub in sorted(pathlib.Path(stub_directory).glob("*.pyi")):
        for node in ast.parse(stub.read_text()).body:
            if isinstance(node, ast.FunctionDef) and hasattr(shinqlx, node.name):
                if inspect.isfunction(getattr(shinqlx, node.name)):
                    continue
                found += differences_of(
                    f"{stub.name}: {node.name}",
                    shape(node.args, False),
                    runtime_shape(getattr(shinqlx, node.name)),
                )
            elif isinstance(node, ast.ClassDef):
                if TYPING_ONLY & {getattr(base, "id", None) for base in node.bases}:
                    continue
                runtime_class = resolve(node.name)
                if runtime_class is not None:
                    found += compare_class(stub.name, node, runtime_class)
    return sorted(set(found))
"#,
                c"",
                c"",
            )
            .and_then(|module| {
                module.call_method1(
                    intern!(py, "differences"),
                    (concat!(env!("CARGO_MANIFEST_DIR"), "/python/shinqlx"),),
                )
            })
            .and_then(|found| found.extract::<Vec<String>>())
            .expect("could not compare the stub signatures with the runtime");

            assert_eq!(
                differences, KNOWN_DIFFERENCES,
                "the differences between stubs and runtime changed"
            );
        });
    }

    const KNOWN_DIFFERENCES: [&str; 1] =
        ["_player.pyi: Player.slay_with_mod: parameter name stub=mod runtime=means_of_death"];
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
                Python::attach(|py| {
                    let result = initialize_cvars(py);
                    assert!(result.is_ok());
                });
            });
    }
}

#[pyfunction(name = "initialize")]
fn initialize(py: Python<'_>) {
    py.detach(register_handlers)
}

fn try_get_plugins_path() -> Result<PathBuf, &'static str> {
    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err("no main engine found")
        },
        |main_engine| {
            main_engine.find_cvar("qlx_pluginsPath").map_or_else(
                || {
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
    fn try_get_plugins_path_with_no_main_engine() {
        let result = try_get_plugins_path();
        assert!(result.is_err_and(|err| err == "no main engine found"));
    }

    #[test]
    #[serial]
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
    Python::initialize();
    Python::attach(|py| {
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

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        error!(target: "shinqlx", "pyshinqlx_finalize was called before being initialized");
        return Err(PythonInitializationError::NotInitializedError);
    }

    CUSTOM_COMMAND_HANDLER.store(None);

    Python::attach(|py| {
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

        let _shinqlx_module = Python::attach(|py| {
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
    use core::{borrow::BorrowMut, ffi::CStr, fmt::Debug};

    use mockall::predicate;
    use pyo3::{intern, prelude::*};

    use super::{
        Command, Flight, Holdable, Player, PlayerInfo, PlayerState, Powerups, Vector3, Weapons,
    };
    use crate::{
        ffi::c::prelude::{
            CVar, CVarBuilder, MockClient, MockGameEntity, clientState_t, cvar_t, privileges_t,
            team_t, weapon_t,
        },
        prelude::MockEngineBuilder,
    };

    /// A mocked engine that knows the given `(name, value)` cvars.
    pub(crate) fn engine_with_cvars(cvars: &[(&'static str, &'static CStr)]) -> MockEngineBuilder {
        cvars
            .iter()
            .fold(MockEngineBuilder::default(), |engine, (name, value)| {
                let name = *name;
                let mut raw_cvar = CVarBuilder::default()
                    .string(value.as_ptr().cast_mut())
                    .build()
                    .expect("this should not happen");
                engine.with_find_cvar(
                    move |cmd| cmd == name,
                    move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                    0..,
                )
            })
    }

    /// Runs the test with the given `(client id, name)` players connected to the mocked engine.
    /// All other client slots up to the highest id are free.
    pub(crate) fn with_connected_players<F>(
        engine: MockEngineBuilder,
        players: &[(i32, &'static str)],
        test: F,
    ) where
        F: FnOnce(),
    {
        let slots = players.iter().map(|(id, _)| id + 1).max().unwrap_or(0);

        let clients = MockClient::from_context();
        for id in 0..slots {
            let connected = players.iter().any(|(player_id, _)| *player_id == id);
            clients
                .expect()
                .with(predicate::eq(id))
                .returning(move |_client_id| {
                    let mut client = MockClient::new();
                    client.expect_get_state().returning(move || {
                        if connected {
                            clientState_t::CS_ACTIVE
                        } else {
                            clientState_t::CS_FREE
                        }
                    });
                    client.expect_get_user_info().returning(|| "asdf".into());
                    client
                        .expect_get_steam_id()
                        .returning(move || 1000 + u64::try_from(id).unwrap_or_default());
                    client
                });
        }

        let entities = MockGameEntity::from_context();
        for (id, name) in players {
            let name = *name;
            entities
                .expect()
                .with(predicate::eq(*id))
                .returning(move |_client_id| {
                    let mut entity = MockGameEntity::new();
                    entity
                        .expect_get_player_name()
                        .returning(move || name.to_string());
                    entity.expect_get_team().returning(|| team_t::TEAM_RED);
                    entity
                        .expect_get_privileges()
                        .returning(|| privileges_t::PRIV_NONE);
                    entity
                });
        }

        let mut engine = engine.with_max_clients(slots);
        engine.run(test);
    }

    pub(crate) struct RestoredAttribute<'py> {
        owner: Bound<'py, PyAny>,
        name: &'static str,
        original: Option<Bound<'py, PyAny>>,
    }

    impl<'py> RestoredAttribute<'py> {
        pub(crate) fn capture(owner: &Bound<'py, PyAny>, name: &'static str) -> Self {
            let original = owner
                .getattr(intern!(owner.py(), "__dict__"))
                .and_then(|attributes| attributes.get_item(name))
                .ok();
            Self {
                owner: owner.clone(),
                name,
                original,
            }
        }
    }

    impl Drop for RestoredAttribute<'_> {
        fn drop(&mut self) {
            let _ = match self.original.take() {
                Some(original) => self.owner.setattr(self.name, original),
                None => self.owner.delattr(self.name),
            };
        }
    }

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
#[allow(
    dead_code,
    reason = "mock module, not every function is called in tests"
)]
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
    use pyo3::{Python, append_to_inittab, ffi::Py_IsInitialized};
    use rstest::fixture;

    use super::pyshinqlx_module;

    #[fixture]
    #[once]
    pub(crate) fn pyshinqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyshinqlx_module);
            Python::initialize();
        }
        Python::attach(|py| {
            py.import("shinqlx").expect("this should not happen");
        });
    }
}
