use core::ffi::{c_char, c_int};

use detour::{RawDetour, TypedDetour};

use super::{
    QuakeLiveEngine, StaticDetours, StaticFunctions,
    mock_quake_functions::*,
    qagame::module::{ModuleLoaderDetours, ModuleLoaderFunctions},
    qzeroded::{
        console::{ConsoleDetours, ConsoleFunctions},
        cvar::{CvarDetours, CvarFunctions},
        server::{ServerDetours, ServerFunctions, SteamPacketHandlerType},
    },
};
use crate::{
    composition::ENGINE_CALLBACKS,
    engine::prelude::{client_t, cvar_t, qboolean, usercmd_t},
};

pub(crate) fn default_static_functions() -> StaticFunctions {
    StaticFunctions {
        console: ConsoleFunctions {
            com_printf_orig: Com_Printf,
            cmd_addcommand_orig: Cmd_AddCommand,
            cmd_args_orig: Cmd_Args,
            cmd_argv_orig: Cmd_Argv,
            cmd_tokenizestring_orig: Cmd_Tokenizestring,
            cbuf_executetext_orig: Cbuf_ExecuteText,
            cmd_executestring_orig: Cmd_ExecuteString,
            cmd_argc_orig: Cmd_Argc,
        },
        cvar: CvarFunctions {
            cvar_findvar_orig: Cvar_FindVar,
            cvar_get_orig: Cvar_Get,
            cvar_getlimit_orig: Cvar_GetLimit,
            cvar_set2_orig: Cvar_Set2,
        },
        server: ServerFunctions {
            sv_sendservercommand_orig: SV_SendServerCommand,
            sv_executeclientcommand_orig: SV_ExecuteClientCommand,
            sv_shutdown_orig: SV_Shutdown,
            sv_map_f_orig: SV_Map_f,
            sv_cliententerworld_orig: SV_ClientEnterWorld,
            sv_setconfigstring_orig: SV_SetConfigstring,
            sv_getconfigstring_orig: SV_GetConfigstring,
            sv_dropclient_orig: SV_DropClient,
            sv_spawnserver_orig: SV_SpawnServer,
            sv_handlesteampacket_orig: Some(SV_HandleSteamPacket),
        },
        module_loader: ModuleLoaderFunctions {
            sys_setmoduleoffset_orig: Sys_SetModuleOffset,
        },
    }
}

pub(crate) fn default_static_detours() -> StaticDetours {
    StaticDetours {
        console: ConsoleDetours {
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            cmd_addcommand_detour: unsafe {
                TypedDetour::new(
                    Cmd_AddCommand as extern "C" fn(*const c_char, unsafe extern "C" fn()),
                    detoured_Cmd_AddCommand,
                )
            }
            .expect("this should not happen"),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            com_printf_detour: unsafe {
                RawDetour::new(Com_Printf as *const (), detoured_Com_Printf as *const ())
            }
            .expect("this should not happen"),
        },
        cvar: CvarDetours {
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            cvar_set2_detour: unsafe {
                TypedDetour::new(
                    Cvar_Set2
                        as extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
                    detoured_Cvar_Set2,
                )
            }
            .expect("this should not happen"),
        },
        server: ServerDetours {
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_executeclientcommand_detour: unsafe {
                TypedDetour::new(
                    SV_ExecuteClientCommand
                        as extern "C" fn(*mut client_t, *const c_char, qboolean),
                    detoured_SV_ExecuteClientCommand,
                )
            }
            .expect("this should not happen"),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_cliententerworld_detour: unsafe {
                TypedDetour::new(
                    SV_ClientEnterWorld as extern "C" fn(*mut client_t, *mut usercmd_t),
                    detoured_SV_ClientEnterWorld,
                )
            }
            .expect("this should not happen"),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_setconfgistring_detour: unsafe {
                TypedDetour::new(
                    SV_SetConfigstring as extern "C" fn(c_int, *const c_char),
                    detoured_SV_SetConfigstring,
                )
            }
            .expect("this should not happen"),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_dropclient_detour: unsafe {
                TypedDetour::new(
                    SV_DropClient as extern "C" fn(*mut client_t, *const c_char),
                    detoured_SV_DropClient,
                )
            }
            .expect("this should not happen"),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_spawnserver_detour: unsafe {
                TypedDetour::new(
                    SV_SpawnServer as extern "C" fn(*mut c_char, qboolean),
                    detoured_SV_SpawnServer,
                )
            }
            .expect("this should not happen"),
            sv_handlesteampacket_detour: Some(
                // SAFETY: the original is a function of the engine and the replacement has the same signature.
                unsafe {
                    TypedDetour::new(
                        SV_HandleSteamPacket as SteamPacketHandlerType,
                        detoured_SV_HandleSteamPacket,
                    )
                }
                .expect("this should not happen"),
            ),
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sv_sendservercommand_detour: unsafe {
                RawDetour::new(
                    SV_SendServerCommand as *const (),
                    detoured_SV_SendServerCommand as *const (),
                )
            }
            .expect("this should not happen"),
        },
        module_loader: ModuleLoaderDetours {
            // SAFETY: the original is a function of the engine and the replacement has the same signature.
            sys_setmoduleoffset_detour: unsafe {
                TypedDetour::new(
                    Sys_SetModuleOffset as extern "C" fn(*mut c_char, unsafe extern "C" fn()),
                    detoured_Sys_SetModuleOffset,
                )
            }
            .expect("this should not happen"),
        },
    }
}

pub(crate) fn default_quake_engine() -> QuakeLiveEngine {
    QuakeLiveEngine::with_callbacks(&ENGINE_CALLBACKS)
}

#[cfg(not(tarpaulin_include))]
pub(crate) unsafe extern "C" fn test_func() {}
