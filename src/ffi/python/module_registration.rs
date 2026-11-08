use core::hint::cold_path;

use pyo3::{
    exceptions::PyEnvironmentError,
    intern,
    types::{IntoPyDict, PyDict, PyTuple},
};

use super::{
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, ConnectionStates,
    DEFAULT_PLUGINS, EVENT_DISPATCHERS, FREE_CHAT_CHANNEL, GameTypes, MODULES, PluginLoadError,
    PluginUnloadError, RED_TEAM_CHAT_CHANNEL, SHINQLX_THREADNAME, SPECTATOR_CHAT_CHANNEL, Teams,
    decorators, delay, events_enum::register_events_enum, get_stats_listener, handlers, initialize,
    initialize_cvars, load_plugin, load_preset_plugins, next_frame, prelude::*,
    pyshinqlx_configure_logger, pyshinqlx_get_logger, pyshinqlx_handle_exception,
    pyshinqlx_handle_threading_exception, pyshinqlx_log_exception, pyshinqlx_owner,
    pyshinqlx_parse_variables, reload_plugin, set_map_subtitles, set_plugins_version, thread,
    try_get_plugins_path, unload_plugin, uptime,
};
use crate::{MAIN_ENGINE, quake_live_engine::FindCVar};

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

/// Initialization that needs to be called after QLDS has finished
/// its own initialization.
#[pyfunction(name = "late_init", pass_module)]
pub(super) fn late_init(module: &Bound<'_, PyModule>) -> PyResult<()> {
    initialize_cvars(module.py())?;

    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
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
                    .map_or_else(
                        || {
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
pub(super) fn pyshinqlx_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
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
    // The constants are exported first because Python functions may use them as keyword defaults.
    m.add("RET_NONE", ReturnCode::NONE as i32)?;
    m.add("RET_STOP", ReturnCode::STOP as i32)?;
    m.add("RET_STOP_EVENT", ReturnCode::STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", ReturnCode::STOP_ALL as i32)?;
    m.add("RET_USAGE", ReturnCode::USAGE as i32)?;
    m.add("PRI_HIGHEST", Priority::HIGHEST as i32)?;
    m.add("PRI_HIGH", Priority::HIGH as i32)?;
    m.add("PRI_NORMAL", Priority::NORMAL as i32)?;
    m.add("PRI_LOW", Priority::LOW as i32)?;
    m.add("PRI_LOWEST", Priority::LOWEST as i32)?;

    m.add("CVAR_ARCHIVE", CvarFlag::ARCHIVE as i32)?;
    m.add("CVAR_USERINFO", CvarFlag::USERINFO as i32)?;
    m.add("CVAR_SERVERINFO", CvarFlag::SERVERINFO as i32)?;
    m.add("CVAR_SYSTEMINFO", CvarFlag::SYSTEMINFO as i32)?;
    m.add("CVAR_INIT", CvarFlag::INIT as i32)?;
    m.add("CVAR_LATCH", CvarFlag::LATCH as i32)?;
    m.add("CVAR_ROM", CvarFlag::ROM as i32)?;
    m.add("CVAR_USER_CREATED", CvarFlag::USER_CREATED as i32)?;
    m.add("CVAR_TEMP", CvarFlag::TEMP as i32)?;
    m.add("CVAR_CHEAT", CvarFlag::CHEAT as i32)?;
    m.add("CVAR_NORESTART", CvarFlag::NORESTART as i32)?;

    m.add("PRIV_NONE", Privileges::NONE as i32)?;
    m.add("PRIV_MOD", Privileges::MOD as i32)?;
    m.add("PRIV_ADMIN", Privileges::ADMIN as i32)?;
    m.add("PRIV_ROOT", Privileges::ROOT as i32)?;
    m.add("PRIV_BANNED", Privileges::BANNED as i32)?;

    m.add("CS_FREE", ConnectionState::FREE as i32)?;
    m.add("CS_ZOMBIE", ConnectionState::ZOMBIE as i32)?;
    m.add("CS_CONNECTED", ConnectionState::CONNECTED as i32)?;
    m.add("CS_PRIMED", ConnectionState::PRIMED as i32)?;
    m.add("CS_ACTIVE", ConnectionState::ACTIVE as i32)?;

    m.add("TEAM_FREE", Team::FREE as i32)?;
    m.add("TEAM_RED", Team::RED as i32)?;
    m.add("TEAM_BLUE", Team::BLUE as i32)?;
    m.add("TEAM_SPECTATOR", Team::SPECTATOR as i32)?;

    m.add("MOD_UNKNOWN", MeansOfDeath::UNKNOWN as i32)?;
    m.add("MOD_SHOTGUN", MeansOfDeath::SHOTGUN as i32)?;
    m.add("MOD_GAUNTLET", MeansOfDeath::GAUNTLET as i32)?;
    m.add("MOD_MACHINEGUN", MeansOfDeath::MACHINEGUN as i32)?;
    m.add("MOD_GRENADE", MeansOfDeath::GRENADE as i32)?;
    m.add("MOD_GRENADE_SPLASH", MeansOfDeath::GRENADE_SPLASH as i32)?;
    m.add("MOD_ROCKET", MeansOfDeath::ROCKET as i32)?;
    m.add("MOD_ROCKET_SPLASH", MeansOfDeath::ROCKET_SPLASH as i32)?;
    m.add("MOD_PLASMA", MeansOfDeath::PLASMA as i32)?;
    m.add("MOD_PLASMA_SPLASH", MeansOfDeath::PLASMA_SPLASH as i32)?;
    m.add("MOD_RAILGUN", MeansOfDeath::RAILGUN as i32)?;
    m.add("MOD_LIGHTNING", MeansOfDeath::LIGHTNING as i32)?;
    m.add("MOD_BFG", MeansOfDeath::BFG as i32)?;
    m.add("MOD_BFG_SPLASH", MeansOfDeath::BFG_SPLASH as i32)?;
    m.add("MOD_WATER", MeansOfDeath::WATER as i32)?;
    m.add("MOD_SLIME", MeansOfDeath::SLIME as i32)?;
    m.add("MOD_LAVA", MeansOfDeath::LAVA as i32)?;
    m.add("MOD_CRUSH", MeansOfDeath::CRUSH as i32)?;
    m.add("MOD_TELEFRAG", MeansOfDeath::TELEFRAG as i32)?;
    m.add("MOD_FALLING", MeansOfDeath::FALLING as i32)?;
    m.add("MOD_SUICIDE", MeansOfDeath::SUICIDE as i32)?;
    m.add("MOD_TARGET_LASER", MeansOfDeath::TARGET_LASER as i32)?;
    m.add("MOD_TRIGGER_HURT", MeansOfDeath::TRIGGER_HURT as i32)?;
    m.add("MOD_NAIL", MeansOfDeath::NAIL as i32)?;
    m.add("MOD_CHAINGUN", MeansOfDeath::CHAINGUN as i32)?;
    m.add("MOD_PROXIMITY_MINE", MeansOfDeath::PROXIMITY_MINE as i32)?;
    m.add("MOD_KAMIKAZE", MeansOfDeath::KAMIKAZE as i32)?;
    m.add("MOD_JUICED", MeansOfDeath::JUICED as i32)?;
    m.add("MOD_GRAPPLE", MeansOfDeath::GRAPPLE as i32)?;
    m.add("MOD_SWITCH_TEAMS", MeansOfDeath::SWITCH_TEAMS as i32)?;
    m.add("MOD_THAW", MeansOfDeath::THAW as i32)?;
    m.add(
        "MOD_LIGHTNING_DISCHARGE",
        MeansOfDeath::LIGHTNING_DISCHARGE as i32,
    )?;
    m.add("MOD_HMG", MeansOfDeath::HMG as i32)?;
    m.add(
        "MOD_RAILGUN_HEADSHOT",
        MeansOfDeath::RAILGUN_HEADSHOT as i32,
    )?;

    m.add("DAMAGE_RADIUS", DamageFlag::RADIUS as i32)?;
    m.add("DAMAGE_NO_ARMOR", DamageFlag::NO_ARMOR as i32)?;
    m.add("DAMAGE_NO_KNOCKBACK", DamageFlag::NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_NO_PROTECTION", DamageFlag::NO_PROTECTION as i32)?;
    m.add(
        "DAMAGE_NO_TEAM_PROTECTION",
        DamageFlag::NO_TEAM_PROTECTION as i32,
    )?;

    m.add_class::<Vector3>()?;
    m.add_class::<Flight>()?;
    m.add_class::<ConfigString>()?;
    m.add_class::<Team>()?;
    m.add_class::<Privileges>()?;
    m.add_class::<ConnectionState>()?;
    m.add_class::<Priority>()?;
    m.add_class::<ReturnCode>()?;
    m.add_class::<VoteState>()?;
    m.add_class::<Weapon>()?;
    m.add_class::<Permission>()?;
    register_events_enum(m)?;
    m.add_class::<GameState>()?;
    m.add_class::<Holdable>()?;
    m.add_class::<CvarFlag>()?;
    m.add_class::<DamageFlag>()?;
    m.add_class::<MeansOfDeath>()?;
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
    m.add(
        "WEAPONS",
        [
            &Weapon::GAUNTLET,
            &Weapon::MACHINEGUN,
            &Weapon::SHOTGUN,
            &Weapon::GRENADE_LAUNCHER,
            &Weapon::ROCKET_LAUNCHER,
            &Weapon::LIGHTNING,
            &Weapon::RAILGUN,
            &Weapon::PLASMAGUN,
            &Weapon::BFG,
            &Weapon::GRAPPLING_HOOK,
            &Weapon::NAILGUN,
            &Weapon::PROX_LAUNCHER,
            &Weapon::CHAINGUN,
            &Weapon::HMG,
            &Weapon::HANDS,
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
    decorators::register_decorators(m)?;
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
        .set_item(intern!(m.py(), "shinqlx.database"), database_module)?;

    Ok(())
}
