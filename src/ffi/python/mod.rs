mod dispatchers;
mod embed;
mod flight;
mod holdable;
mod player_info;
mod player_state;
mod player_stats;
mod powerups;
mod vector3;
mod weapons;

pub(crate) mod prelude {
    pub(crate) use super::embed::*;
    pub(crate) use super::flight::Flight;
    pub(crate) use super::holdable::Holdable;
    pub(crate) use super::player_info::PlayerInfo;
    pub(crate) use super::player_state::PlayerState;
    pub(crate) use super::player_stats::PlayerStats;
    pub(crate) use super::powerups::Powerups;
    pub(crate) use super::vector3::Vector3;
    pub(crate) use super::weapons::Weapons;

    pub(crate) use super::{
        ALLOW_FREE_CLIENT, CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, CUSTOM_COMMAND_HANDLER,
        DAMAGE_HANDLER, FRAME_HANDLER, KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER,
        NEW_GAME_HANDLER, PLAYER_CONNECT_HANDLER, PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER,
        PLAYER_SPAWN_HANDLER, RCON_HANDLER, SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
    };

    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize_context, pyshinqlx_is_initialized_context, pyshinqlx_reload_context,
    };
    pub(crate) use super::PythonInitializationError;
    #[cfg(not(test))]
    pub(crate) use super::{pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload};

    #[cfg(not(test))]
    pub(crate) use super::dispatchers::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
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
    pub(crate) use super::pyshinqlx_setup_fixture::*;

    pub(crate) use pyo3::prelude::*;
}

use crate::ffi::c::prelude::*;
use prelude::*;

use arc_swap::ArcSwapOption;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use log::*;
use once_cell::sync::Lazy;
use pyo3::{append_to_inittab, intern, prepare_freethreaded_python};

pub(crate) static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) static CLIENT_COMMAND_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static SERVER_COMMAND_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static FRAME_HANDLER: Lazy<ArcSwapOption<PyObject>> = Lazy::new(ArcSwapOption::empty);
pub(crate) static PLAYER_CONNECT_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static PLAYER_LOADED_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static PLAYER_DISCONNECT_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static CUSTOM_COMMAND_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static NEW_GAME_HANDLER: Lazy<ArcSwapOption<PyObject>> = Lazy::new(ArcSwapOption::empty);
pub(crate) static SET_CONFIGSTRING_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static RCON_HANDLER: Lazy<ArcSwapOption<PyObject>> = Lazy::new(ArcSwapOption::empty);
pub(crate) static CONSOLE_PRINT_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static PLAYER_SPAWN_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static KAMIKAZE_USE_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static KAMIKAZE_EXPLODE_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);
pub(crate) static DAMAGE_HANDLER: Lazy<ArcSwapOption<PyObject>> = Lazy::new(ArcSwapOption::empty);

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
fn pyshinqlx_root_module(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}

#[pymodule]
#[pyo3(name = "_shinqlx")]
fn pyshinqlx_module(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
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
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
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
    .iter()
    .for_each(|&handler_lock| handler_lock.store(None));

    let reinit_result = Python::with_gil(|py| {
        let importlib_module = py.import_bound("importlib")?;
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
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
pub(crate) mod pyshinqlx_setup_fixture {
    use super::pyshinqlx_module;

    use pyo3::{append_to_inittab, ffi::Py_IsInitialized, prepare_freethreaded_python};
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
