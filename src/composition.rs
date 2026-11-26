//! Wires the engine to the code that reacts to it.

#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
use crate::hooks::shinqlx_sv_handlesteampacket;
use crate::{
    commands::{
        cmd_center_print, cmd_plugin_commands, cmd_py_command, cmd_py_rcon, cmd_regular_print,
        cmd_reliable_guard, cmd_restart_python, cmd_scoreboard_trim, cmd_send_server_command,
        cmd_slap, cmd_slay,
    },
    engine::callbacks::*,
    hooks::{
        ShiNQlx_Com_Printf, ShiNQlx_SV_SendServerCommand, shinqlx_client_connect,
        shinqlx_clientspawn, shinqlx_cmd_addcommand, shinqlx_cvar_set2, shinqlx_fire_weapon,
        shinqlx_g_damage, shinqlx_g_initgame, shinqlx_g_runframe, shinqlx_g_shutdowngame,
        shinqlx_g_startkamikaze, shinqlx_stats_flag_status, shinqlx_stats_medal,
        shinqlx_sv_cliententerworld, shinqlx_sv_dropclient, shinqlx_sv_executeclientcommand,
        shinqlx_sv_setconfigstring, shinqlx_sv_spawnserver, shinqlx_sys_setmoduleoffset,
        shinqlx_touch_item,
    },
    python::prelude::pyshinqlx_initialize,
};

pub(crate) static ENGINE_CALLBACKS: EngineCallbacks = EngineCallbacks {
    console: ConsoleCallbacks {
        cmd_addcommand: shinqlx_cmd_addcommand,
        com_printf: ShiNQlx_Com_Printf,
    },
    cvar: CvarCallbacks {
        cvar_set2: shinqlx_cvar_set2,
    },
    server: ServerCallbacks {
        sv_executeclientcommand: shinqlx_sv_executeclientcommand,
        sv_cliententerworld: shinqlx_sv_cliententerworld,
        sv_sendservercommand: ShiNQlx_SV_SendServerCommand,
        sv_setconfigstring: shinqlx_sv_setconfigstring,
        sv_dropclient: shinqlx_sv_dropclient,
        sv_spawnserver: shinqlx_sv_spawnserver,
        #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
        sv_handlesteampacket: shinqlx_sv_handlesteampacket,
    },
    module_loader: ModuleLoaderCallbacks {
        sys_setmoduleoffset: shinqlx_sys_setmoduleoffset,
    },
    vm: VmCallbacks {
        g_initgame: shinqlx_g_initgame,
        g_runframe: shinqlx_g_runframe,
        g_shutdowngame: shinqlx_g_shutdowngame,
    },
    game: GameCallbacks {
        client_connect: shinqlx_client_connect,
        client_spawn: shinqlx_clientspawn,
    },
    entity: EntityCallbacks {
        g_startkamikaze: shinqlx_g_startkamikaze,
        g_damage: shinqlx_g_damage,
        fire_weapon: shinqlx_fire_weapon,
        stats_medal: shinqlx_stats_medal,
        stats_flag_status: shinqlx_stats_flag_status,
    },
    item: ItemCallbacks {
        touch_item: shinqlx_touch_item,
    },
    console_commands: &[
        ("cmd", cmd_send_server_command),
        ("cp", cmd_center_print),
        ("print", cmd_regular_print),
        ("slap", cmd_slap),
        ("slay", cmd_slay),
        ("qlx", cmd_py_rcon),
        ("pycmd", cmd_py_command),
        ("pyrestart", cmd_restart_python),
        ("reliable", cmd_reliable_guard),
        ("scoreboard", cmd_scoreboard_trim),
        ("plugincmds", cmd_plugin_commands),
    ],
    set_configstring: shinqlx_set_configstring,
    initialize_scripting: pyshinqlx_initialize,
};
