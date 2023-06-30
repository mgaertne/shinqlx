#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types)]
#![feature(c_variadic)]

#[cfg(test)]
#[macro_use]
extern crate hamcrest;

macro_rules! debug_println {
    () => {
        println!("{}", crate::DEBUG_PRINT_PREFIX)
    };
    ($($arg:tt)*) => {
        println!("{} {}", crate::DEBUG_PRINT_PREFIX, $($arg)*)
    };
}

mod activator;
mod client;
mod commands;
mod current_level;
mod cvar;
mod game_client;
mod game_entity;
mod game_item;
mod hooks;
mod patches;
mod pyminqlx;
mod quake_live_engine;
mod quake_live_functions;
mod quake_types;
mod server_static;

use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
use crate::hooks::hook_static;
use crate::pyminqlx::pyminqlx_initialize;
use crate::quake_live_engine::{AddCommand, FindCVar, QuakeLiveEngine};
use crate::quake_live_functions::{pattern_search, pattern_search_module};
use crate::quake_types::{cbufExec_t, client_t, cvar_t, qboolean, usercmd_t};
use crate::PyMinqlx_InitStatus_t::PYM_SUCCESS;
use ctor::ctor;
use once_cell::race::OnceBool;
use once_cell::sync::OnceCell;
use procfs::process::{MMapPath, MemoryMap, Process};
use quake_live_functions::QuakeLiveFunction;
use std::env::args;
use std::ffi::{c_char, c_int, c_void, OsStr};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

pub(crate) const DEBUG_PRINT_PREFIX: &str = "[shinqlx]";

pub(crate) const SV_TAGS_PREFIX: &str = "shinqlx";

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum PyMinqlx_InitStatus_t {
    PYM_SUCCESS,
    PYM_PY_INIT_ERROR,
    PYM_MAIN_SCRIPT_ERROR,
    PYM_ALREADY_INITIALIZED,
    PYM_NOT_INITIALIZED_ERROR,
}

pub(crate) static COMMON_INITIALIZED: OnceBool = OnceBool::new();
pub(crate) static CVARS_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub(crate) static SV_MAXCLIENTS: AtomicI32 = AtomicI32::new(0);
pub(crate) static ALLOW_FREE_CLIENT: AtomicI32 = AtomicI32::new(-1);

// Currently called by My_Cmd_AddCommand(), since it's called at a point where we
// can safely do whatever we do below. It'll segfault if we do it at the entry
// point, since functions like Cmd_AddCommand need initialization first.
fn initialize_static() {
    debug_println!("Initializing...");
    let quake_live_engine = QuakeLiveEngine::default();
    quake_live_engine.add_command("cmd", cmd_send_server_command);
    quake_live_engine.add_command("cp", cmd_center_print);
    quake_live_engine.add_command("print", cmd_regular_print);
    quake_live_engine.add_command("slap", cmd_slap);
    quake_live_engine.add_command("slay", cmd_slay);
    quake_live_engine.add_command("qlx", cmd_py_rcon);
    quake_live_engine.add_command("pycmd", cmd_py_command);
    quake_live_engine.add_command("pyrestart", cmd_restart_python);

    let res = pyminqlx_initialize();

    if res != PYM_SUCCESS {
        panic!("Python initialization failed.");
    }

    COMMON_INITIALIZED.set(true).unwrap();
}

// Called after the game is initialized.
fn initialize_cvars() {
    let Some(maxclients) = QuakeLiveEngine::default().find_cvar("sv_maxclients") else {
        return;
    };

    SV_MAXCLIENTS.store(maxclients.get_integer(), Ordering::Relaxed);
    CVARS_INITIALIZED.store(true, Ordering::Relaxed);
}

#[cfg(target_pointer_width = "64")]
const QZERODED: &str = "qzeroded.x64";
#[cfg(target_pointer_width = "32")]
const QZERODED: &str = "qzeroded.x86";

#[ctor]
fn initialize() {
    if let Some(progname) = args().next() {
        if !progname.ends_with(QZERODED) {
            return;
        }
    } else {
        return;
    }

    search_static_functions();

    debug_println!("Shared library loaded");
    if let Err(res) = hook_static() {
        debug_println!(format!("ERROR: failed to hook static methods: {}", res));
        debug_println!("Exiting.");
    };
}

type CvarGetLimitType =
    fn(*const c_char, *const c_char, *const c_char, *const c_char, c_int) -> *const cvar_t;

pub(crate) static COM_PRINTF_ORIG_PTR: OnceCell<extern "C" fn(*const c_char, ...)> =
    OnceCell::new();
pub(crate) static CMD_ADDCOMMAND_ORIG_PTR: OnceCell<fn(*const c_char, unsafe extern "C" fn())> =
    OnceCell::new();
pub(crate) static CMD_ARGS_ORIG_PTR: OnceCell<fn() -> *const c_char> = OnceCell::new();
pub(crate) static CMD_ARGV_ORIG_PTR: OnceCell<fn(c_int) -> *const c_char> = OnceCell::new();
pub(crate) static CMD_TOKENIZESTRING_ORIG_PTR: OnceCell<fn(*const c_char)> = OnceCell::new();
pub(crate) static CBUF_EXECUTETEXT_ORIG_PTR: OnceCell<fn(cbufExec_t, *const c_char)> =
    OnceCell::new();
pub(crate) static CVAR_FINDVAR_ORIG_PTR: OnceCell<fn(*const c_char) -> *const cvar_t> =
    OnceCell::new();
pub(crate) static CVAR_GET_ORIG_PTR: OnceCell<
    fn(*const c_char, *const c_char, c_int) -> *const cvar_t,
> = OnceCell::new();
pub(crate) static CVAR_GETLIMIT_ORIG_PTR: OnceCell<CvarGetLimitType> = OnceCell::new();
pub(crate) static CVAR_SET2_ORIG_PTR: OnceCell<
    fn(*const c_char, *const c_char, qboolean) -> *const cvar_t,
> = OnceCell::new();
pub(crate) static SV_SENDSERVERCOMMAND_ORIG_PTR: OnceCell<
    extern "C" fn(*mut client_t, *const c_char, ...),
> = OnceCell::new();
pub(crate) static SV_EXECUTECLIENTCOMMAND_ORIG_PTR: OnceCell<
    fn(*mut client_t, *const c_char, qboolean),
> = OnceCell::new();
pub(crate) static SV_SHUTDOWN_ORIG_PTR: OnceCell<fn(*const c_char)> = OnceCell::new();
pub(crate) static SV_MAP_F_ORIG_PTR: OnceCell<fn()> = OnceCell::new();
pub(crate) static SV_CLIENTENTERWORLD_ORIG_PTR: OnceCell<fn(*mut client_t, *mut usercmd_t)> =
    OnceCell::new();
pub(crate) static SV_SETCONFIGSTRING_ORIG_PTR: OnceCell<fn(c_int, *const c_char)> = OnceCell::new();
pub(crate) static SV_GETCONFIGSTRING_ORIG_PTR: OnceCell<fn(c_int, *const c_char, c_int)> =
    OnceCell::new();
pub(crate) static SV_DROPCLIENT_ORIG_PTR: OnceCell<fn(*mut client_t, *const c_char)> =
    OnceCell::new();
pub(crate) static SYS_SETMODULEOFFSET_ORIG_PTR: OnceCell<
    fn(*const c_char, unsafe extern "C" fn()),
> = OnceCell::new();
pub(crate) static SV_SPAWNSERVER_ORIG_PTR: OnceCell<fn(*const c_char, qboolean)> = OnceCell::new();
pub(crate) static CMD_EXECUTESTRING_ORIG_PTR: OnceCell<fn(*const c_char)> = OnceCell::new();
pub(crate) static CMD_ARGC_ORIG_PTR: OnceCell<fn() -> c_int> = OnceCell::new();

pub(crate) fn search_static_functions() {
    let qzeroded_os_str = OsStr::new(QZERODED);
    let Ok(myself_process) = Process::myself() else {
        panic!("could not find my own process\n");
    };
    let Ok(myself_maps) = myself_process.maps() else {
        panic!("no memory mapping information found\n");
    };
    let qzeroded_maps: Vec<&MemoryMap> = myself_maps
        .memory_maps
        .iter()
        .filter(|mmap| {
            if let MMapPath::Path(path) = &mmap.pathname {
                path.file_name() == Some(qzeroded_os_str)
            } else {
                false
            }
        })
        .collect();

    if qzeroded_maps.is_empty() {
        debug_println!(format!(
            "no memory mapping information for {} found",
            QZERODED
        ));
        panic!("no memory mapping information found\n");
    }

    debug_println!("Searching for necessary functions...");
    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Com_Printf) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Com_Printf
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Com_Printf, result));
    let original_func = unsafe { std::mem::transmute(result) };
    COM_PRINTF_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_AddCommand) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cmd_AddCommand
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cmd_AddCommand,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CMD_ADDCOMMAND_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Args) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cmd_Args
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cmd_Args, result));
    let original_func = unsafe { std::mem::transmute(result) };
    CMD_ARGS_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Argv) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cmd_Argv
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cmd_Argv, result));
    let original_func = unsafe { std::mem::transmute(result) };
    CMD_ARGV_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Tokenizestring) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cmd_Tokenizestring
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cmd_Tokenizestring,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CMD_TOKENIZESTRING_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cbuf_ExecuteText) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cbuf_ExecuteText
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cbuf_ExecuteText,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CBUF_EXECUTETEXT_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_FindVar) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cvar_FindVar
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cvar_FindVar,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CVAR_FINDVAR_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_Get) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cvar_Get
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cvar_Get, result));
    let original_func = unsafe { std::mem::transmute(result) };
    CVAR_GET_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_GetLimit) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cvar_GetLimit
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cvar_GetLimit,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CVAR_GETLIMIT_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_Set2) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Cvar_Set2
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cvar_Set2, result));
    let original_func = unsafe { std::mem::transmute(result) };
    CVAR_SET2_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SendServerCommand) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_SendServerCommand
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_SendServerCommand,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_SENDSERVERCOMMAND_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_ExecuteClientCommand) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_ExecuteClientCommand
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_ExecuteClientCommand,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_EXECUTECLIENTCOMMAND_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_Shutdown) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_Shutdown
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_Shutdown,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_SHUTDOWN_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_Map_f) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_Map_f
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::SV_Map_f, result));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_MAP_F_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_ClientEnterWorld) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_ClientEnterWorld
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_ClientEnterWorld,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_CLIENTENTERWORLD_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SetConfigstring) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_SetConfigstring
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_SetConfigstring,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_SETCONFIGSTRING_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_GetConfigstring) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_GetConfigstring
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_GetConfigstring,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_GETCONFIGSTRING_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_DropClient) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_DropClient
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_DropClient,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_DROPCLIENT_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Sys_SetModuleOffset) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::Sys_SetModuleOffset
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Sys_SetModuleOffset,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SYS_SETMODULEOFFSET_ORIG_PTR.set(original_func).unwrap();

    let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SpawnServer) else
    {
        debug_println!(format!(
            "Function {} not found",
            &QuakeLiveFunction::SV_SpawnServer
        ));
        panic!("Static function not found. Exiting.");
    };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::SV_SpawnServer,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    SV_SPAWNSERVER_ORIG_PTR.set(original_func).unwrap();

    let Some(result) =
            pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_ExecuteString) else
        {
            debug_println!(format!(
                "Function {} not found",
                &QuakeLiveFunction::Cmd_ExecuteString
            ));
            panic!("Static function not found. Exiting.");
        };
    debug_println!(format!(
        "{}: {:#X}",
        &QuakeLiveFunction::Cmd_ExecuteString,
        result
    ));
    let original_func = unsafe { std::mem::transmute(result) };
    CMD_EXECUTESTRING_ORIG_PTR.set(original_func).unwrap();

    // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
    if let Some(sv_map_f_ptr) = SV_MAP_F_ORIG_PTR.get() {
        let base_address =
            unsafe { std::ptr::read_unaligned((*sv_map_f_ptr as usize + 0x81) as *const i32) };
        #[allow(clippy::fn_to_numeric_cast_with_truncation)]
        let cmd_argc_ptr = base_address + *sv_map_f_ptr as i32 + 0x81 + 4;
        debug_println!(format!(
            "{}: {:#X}",
            QuakeLiveFunction::Cmd_Argc,
            cmd_argc_ptr
        ));
        let original_func = unsafe { std::mem::transmute(cmd_argc_ptr as u64) };
        CMD_ARGC_ORIG_PTR.set(original_func).unwrap();
    }
}

pub(crate) static G_ADDEVENT_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static CHECK_PRIVILEGES_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static CLIENT_CONNECT_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static CLIENT_SPAWN_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_DAMAGE_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static TOUCH_ITEM_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static LAUNCH_ITEM_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static DROP_ITEM_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_START_KAMIKAZE_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_FREE_ENTITY_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_INIT_GAME_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_SHUTDOWN_GAME_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_RUN_FRAME_ORIG_PTR: AtomicU64 = AtomicU64::new(0);
pub(crate) static CMD_CALLVOTE_F_ORIG_PTR: AtomicU64 = AtomicU64::new(0);

pub(crate) fn search_vm_functions(qagame: u64, qagame_dllentry: u64) {
    extern "C" {
        fn SearchVmFunctions(qagame: *const c_void, qagame_dllentry: *const c_void) -> c_int;
    }

    let c_result =
        unsafe { SearchVmFunctions(qagame as *const c_void, qagame_dllentry as *const c_void) };
    if c_result != 0 {
        debug_println!("Something went wrong on the C side...");
    }
    debug_println!("Searching for necessary VM functions...");

    for (ql_func, orig_ptr) in [
        (QuakeLiveFunction::G_AddEvent, &G_ADDEVENT_ORIG_PTR),
        (
            QuakeLiveFunction::CheckPrivileges,
            &CHECK_PRIVILEGES_ORIG_PTR,
        ),
        (QuakeLiveFunction::ClientConnect, &CLIENT_CONNECT_ORIG_PTR),
        (QuakeLiveFunction::ClientSpawn, &CLIENT_SPAWN_ORIG_PTR),
        (QuakeLiveFunction::G_Damage, &G_DAMAGE_ORIG_PTR),
        (QuakeLiveFunction::Touch_Item, &TOUCH_ITEM_ORIG_PTR),
        (QuakeLiveFunction::LaunchItem, &LAUNCH_ITEM_ORIG_PTR),
        (QuakeLiveFunction::Drop_Item, &DROP_ITEM_ORIG_PTR),
        (
            QuakeLiveFunction::G_StartKamikaze,
            &G_START_KAMIKAZE_ORIG_PTR,
        ),
        (QuakeLiveFunction::G_FreeEntity, &G_FREE_ENTITY_ORIG_PTR),
        (QuakeLiveFunction::Cmd_Callvote_f, &CMD_CALLVOTE_F_ORIG_PTR),
    ] {
        if let Some(result) = pattern_search(qagame + 0xB000, qagame + 0xB000 + 0xB0000, &ql_func) {
            debug_println!(format!("{}: {:#X}", ql_func, result));
            orig_ptr.store(result, Ordering::Relaxed);
        } else {
            debug_println!(format!("VM function {} not found", ql_func));
            panic!("VM function not found. Exiting.");
        }
    }
}
