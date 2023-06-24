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
mod pyminqlx;
mod quake_live_engine;
mod quake_types;

use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
use crate::hooks::hook_static;
use crate::pyminqlx::pyminqlx_initialize;
use crate::quake_live_engine::{AddCommand, FindCVar, QuakeLiveEngine};
use crate::PyMinqlx_InitStatus_t::PYM_SUCCESS;
use ctor::ctor;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::env::args;
use std::ffi::{c_int, OsStr};
use std::fmt::{Display, Formatter};

pub(crate) const DEBUG_PRINT_PREFIX: &str = "[shinqlx]";

pub(crate) const SV_TAGS_PREFIX: &str = "shinqlx";

#[allow(non_camel_case_types)]
#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum PyMinqlx_InitStatus_t {
    PYM_SUCCESS,
    PYM_PY_INIT_ERROR,
    PYM_MAIN_SCRIPT_ERROR,
    PYM_ALREADY_INITIALIZED,
    PYM_NOT_INITIALIZED_ERROR,
}

pub(crate) static mut COMMON_INITIALIZED: bool = false;
pub(crate) static mut CVARS_INITIALIZED: bool = false;
pub(crate) static mut SV_MAXCLIENTS: i32 = 0;
pub(crate) static mut ALLOW_FREE_CLIENT: i32 = -1;

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
        debug_println!("Python initialization failed.");
        panic!("Python initialization failed.");
    }

    unsafe { COMMON_INITIALIZED = true };
}

// Called after the game is initialized.
fn initialize_cvars() {
    let Some(maxclients) = QuakeLiveEngine::default().find_cvar("sv_maxclients") else {
        return;
    };

    unsafe { SV_MAXCLIENTS = maxclients.get_integer() };
    unsafe { CVARS_INITIALIZED = true };
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

static mut STATIC_FUNCTION_MAP: Lazy<HashMap<QuakeLiveFunction, u64>> =
    Lazy::new(|| HashMap::new());

pub(crate) fn search_static_functions() {
    let qzeroded_os_str = OsStr::new(QZERODED);
    let myself_module = procfs::process::Process::myself().unwrap();
    let myself_maps = myself_module.maps().unwrap();
    let qzeroded_maps: Vec<&procfs::process::MemoryMap> = myself_maps
        .memory_maps
        .iter()
        .filter(|mmap| {
            if let procfs::process::MMapPath::Path(path) = &mmap.pathname {
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

    for ql_func in [
        QuakeLiveFunction::Com_Printf,
        QuakeLiveFunction::Cmd_AddCommand,
        QuakeLiveFunction::Cmd_Args,
        QuakeLiveFunction::Cmd_Argv,
        QuakeLiveFunction::Cmd_Tokenizestring,
        QuakeLiveFunction::Cbuf_ExecuteText,
        QuakeLiveFunction::Cvar_FindVar,
        QuakeLiveFunction::Cvar_Get,
        QuakeLiveFunction::Cvar_GetLimit,
        QuakeLiveFunction::Cvar_Set2,
        QuakeLiveFunction::SV_SendServerCommand,
        QuakeLiveFunction::SV_ExecuteClientCommand,
        QuakeLiveFunction::SV_Shutdown,
        QuakeLiveFunction::SV_Map_f,
        QuakeLiveFunction::SV_ClientEnterWorld,
        QuakeLiveFunction::SV_SetConfigstring,
        QuakeLiveFunction::SV_GetConfigstring,
        QuakeLiveFunction::SV_DropClient,
        QuakeLiveFunction::Sys_SetModuleOffset,
        QuakeLiveFunction::SV_SpawnServer,
        QuakeLiveFunction::Cmd_ExecuteString,
    ] {
        if let Some(result) = pattern_search_module(&qzeroded_maps, &ql_func) {
            debug_println!(format!("{}: {:#X}", &ql_func, result));
            unsafe { STATIC_FUNCTION_MAP.insert(ql_func, result) };
        } else {
            debug_println!(format!("Function {} not found", ql_func));
            panic!("Static function not found. Exiting.");
        }
    }

    // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
    if let Some(sv_map_f_ptr) = unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::SV_Map_f) } {
        let base_address: i32 =
            unsafe { std::ptr::read_unaligned((sv_map_f_ptr + 0x81) as *const i32) };
        let cmd_argc_ptr = base_address + *sv_map_f_ptr as i32 + 0x81 + 4;
        debug_println!(format!(
            "{}: {:#X}",
            QuakeLiveFunction::Cmd_Argc,
            cmd_argc_ptr
        ));
        unsafe { STATIC_FUNCTION_MAP.insert(QuakeLiveFunction::Cmd_Argc, cmd_argc_ptr as u64) };
    }

    extern "C" {
        fn SearchFunctions() -> c_int;
    }
    let search_functions = unsafe { SearchFunctions() };
    if search_functions != 0 {
        panic!("Exiting.");
    }
}

pub(crate) fn pattern_search_module(
    module_info: &Vec<&procfs::process::MemoryMap>,
    ql_func: &QuakeLiveFunction,
) -> Option<u64> {
    for memory_map in module_info {
        if !memory_map
            .perms
            .contains(procfs::process::MMPermissions::READ)
        {
            continue;
        }
        let result = pattern_search(memory_map.address.0, memory_map.address.1, &ql_func);
        if result.is_some() {
            return result;
        }
    }
    None
}

fn pattern_search(start: u64, end: u64, ql_func: &QuakeLiveFunction) -> Option<u64> {
    let pattern = ql_func.pattern();
    let mask = ql_func.mask();
    for i in start..end {
        for j in 0..pattern.len() {
            let char: u8 = unsafe { std::ptr::read((i as usize + j) as *const u8) };
            let pattern_char: u8 = pattern[j];
            let mask_char: u8 = mask[j];
            if mask_char == b'X' && pattern_char != char {
                break;
            } else if j + 1 < mask.len() {
                continue;
            }
            return Some(i);
        }
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[allow(non_camel_case_types)]
pub(crate) enum QuakeLiveFunction {
    Com_Printf,
    Cmd_AddCommand,
    Cmd_Args,
    Cmd_Argv,
    Cmd_Argc,
    Cmd_Tokenizestring,
    Cbuf_ExecuteText,
    Cvar_FindVar,
    Cvar_Get,
    Cvar_GetLimit,
    Cvar_Set2,
    SV_SendServerCommand,
    SV_ExecuteClientCommand,
    SV_Shutdown,
    SV_Map_f,
    SV_ClientEnterWorld,
    SV_SetConfigstring,
    SV_GetConfigstring,
    SV_DropClient,
    Sys_SetModuleOffset,
    SV_SpawnServer,
    Cmd_ExecuteString,
}

impl Display for QuakeLiveFunction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            QuakeLiveFunction::Com_Printf => f.write_str("Com_Printf"),
            QuakeLiveFunction::Cmd_AddCommand => f.write_str("Cmd_AddCommand"),
            QuakeLiveFunction::Cmd_Args => f.write_str("Cmd_Args"),
            QuakeLiveFunction::Cmd_Argv => f.write_str("Cmd_Argv"),
            QuakeLiveFunction::Cmd_Argc => f.write_str("Cmd_Argc"),
            QuakeLiveFunction::Cmd_Tokenizestring => f.write_str("Cmd_Tokenizestring"),
            QuakeLiveFunction::Cbuf_ExecuteText => f.write_str("Cbuf_ExecuteText"),
            QuakeLiveFunction::Cvar_FindVar => f.write_str("Cvar_FindVar"),
            QuakeLiveFunction::Cvar_Get => f.write_str("Cvar_Get"),
            QuakeLiveFunction::Cvar_GetLimit => f.write_str("Cvar_GetLimit"),
            QuakeLiveFunction::Cvar_Set2 => f.write_str("Cvar_Set2"),
            QuakeLiveFunction::SV_SendServerCommand => f.write_str("SV_SendServerCommand"),
            QuakeLiveFunction::SV_ExecuteClientCommand => f.write_str("SV_ExecuteClientCommand"),
            QuakeLiveFunction::SV_Shutdown => f.write_str("SV_Shutdown"),
            QuakeLiveFunction::SV_Map_f => f.write_str("SV_Map_f"),
            QuakeLiveFunction::SV_ClientEnterWorld => f.write_str("SV_ClientEnterWorld"),
            QuakeLiveFunction::SV_SetConfigstring => f.write_str("SV_SetConfigstring"),
            QuakeLiveFunction::SV_GetConfigstring => f.write_str("SV_GetConfigstring"),
            QuakeLiveFunction::SV_DropClient => f.write_str("SV_DropClient"),
            QuakeLiveFunction::Sys_SetModuleOffset => f.write_str("Sys_SetModuleOffset"),
            QuakeLiveFunction::SV_SpawnServer => f.write_str("SV_SpawnServer"),
            QuakeLiveFunction::Cmd_ExecuteString => f.write_str("Cmd_ExecuteString"),
        }
    }
}
impl QuakeLiveFunction {
    pub(crate) fn pattern(&self) -> &[u8] {
        match self {
            QuakeLiveFunction::Com_Printf => b"\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x84\xc0\x48\x89\xb4\x24\x00\x00\x00\x00\x48\x89\x94\x24\x00\x00\x00\x00\x48\x89\x8c\x24\x00\x00\x00\x00\x4c\x89\x84\x24\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_AddCommand => b"\x41\x55\x49\x89\xf5\x41\x54\x49\x89\xfc\x55\x53\x48\x83\xec\x00\x48\x8b\x1d\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x66\x90\x48\x8b\x1b\x48\x85\xdb\x74\x00\x48\x8b\x73\x00\x4c\x89\xe7",
            QuakeLiveFunction::Cmd_Args => b"\x8b\x05\x00\x00\x00\x00\xc6\x05\x00\x00\x00\x00\x00\x83\xf8\x00\x0f\x8e\x00\x00\x00\x00\x41\x54\x44\x8d\x60\x00\x83\xe8\x00\x55\x48\x8d\x68\x00\x53\x31\xdb\x66\x0f\x1f\x84\x00\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_Argv => b"\x3b\x3d\x00\x00\x00\x00\xb8\x00\x00\x00\x00\x73\x00\x48\x63\xff\x48\x8b\x04\xfd\x00\x00\x00\x00\xf3\xc3",
            QuakeLiveFunction::Cmd_Argc => b"\x8b\x05\x00\x00\x00\x00\xc3",
            QuakeLiveFunction::Cmd_Tokenizestring => b"\x48\x85\xff\x53\xc7\x05\x00\x00\x44\x00\x00\x00\x00\x00\x48\x89\xfb\x0f\x84\x00\x00\x00\x00\x48\x89\xfe\xba\x00\x00\x00\x00\xbf\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x8b\x0d\x00\x00\x00\x00",
            QuakeLiveFunction::Cbuf_ExecuteText => b"\x83\xff\x00\x74\x00\x83\xff\x00\x74\x00\x85\xff\x74\x00\xbe\x00\x00\x00\x00\x31\xff\x31\xc0\xe9\x00\x00\x00\x00\x0f\x1f\x40\x00\x48\x85\xf6\x74\x00\x80\x3e\x00\x75\x00\xe9\x00\x00\x00\x00\x90",
            QuakeLiveFunction::Cvar_FindVar => b"\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x8b\x1c\xc5\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x0f\x1f\x00\x48\x8b\x5b\x00\x48\x85\xdb\x74\x00\x48\x8b\x33\x48\x89\xef",
            QuakeLiveFunction::Cvar_Get => b"\x41\x56\x48\x85\xff\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xf4\x55\x48\x89\xfd\x53\x0f\x84\x00\x00\x00\x00\x48\x85\xf6\x0f\x84\x00\x00\x00\x00\x48\x89\xef\xe8\x00\x00\x00\x00\x85\xc0",
            QuakeLiveFunction::Cvar_GetLimit => b"\x41\x57\x45\x89\xc7\x41\x56\x49\x89\xd6\x41\x55\x49\x89\xcd\x41\x54\x49\x89\xf4\x31\xf6\x55\x48\x89\xfd\x48\x89\xd7\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x66\x0f\x14\xc0\x31\xf6\x4c\x89\xef",
            QuakeLiveFunction::Cvar_Set2 => b"\x41\x57\x31\xc0\x41\x56\x41\x89\xd6\x48\x89\xf2\x41\x55\x41\x54\x49\x89\xf4\x48\x89\xfe\x55\x48\x89\xfd\xbf\x00\x00\x00\x00\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x89\xef\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::SV_SendServerCommand => b"\x41\x55\x41\x54\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x84\xc0\x48\x89\x94\x24\x00\x00\x00\x00\x48\x89\x8c\x24\x00\x00\x00\x00\x4c\x89\x84\x24\x00\x00\x00\x00\x4c\x89\x8c\x24\x00\x00\x00\x00",
            QuakeLiveFunction::SV_ExecuteClientCommand => b"\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xfc\x48\x89\xf7\x55\xbd\x00\x00\x00\x00\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x8b\x1d\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xe9\x00\x00\x00\x00\x66\x90",
            QuakeLiveFunction::SV_Shutdown => b"\x48\x8b\x05\x00\x00\x00\x00\x48\x85\xc0\x74\x00\x44\x8b\x50\x00\x45\x85\xd2\x75\x00\xf3\xc3",
            QuakeLiveFunction::SV_Map_f => b"\x41\x55\xbf\x00\x00\x00\x00\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00\xbf\x00\x00\x00\x00\x48\x89\xc5",
            QuakeLiveFunction::SV_ClientEnterWorld => b"\x41\x55\x31\xc0\x49\xbd\x00\x00\x00\x00\x00\x00\x00\x00\x41\x54\x49\x89\xf4\x48\x8d\xb7\x00\x00\x00\x00\x55\x53\x48\x89\xfb\xbf\x00\x00\x00\x00\x48\x89\xdd\x48\x83\xec\x00\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::SV_SetConfigstring => b"\x41\x57\x41\x56\x41\x55\x41\x54\x41\x89\xfc\x55\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x81\xff\x00\x00\x00\x00\x48\x89\x74\x24\x00",
            QuakeLiveFunction::SV_GetConfigstring => b"\x41\x54\x85\xd2\x49\x89\xf4\x55\x89\xd5\x53\x48\x63\xdf\x7e\x00\x81\xfb\x00\x00\x00\x00\x77\x00\x48\x8b\x34\xdd\x00\x00\x00\x00\x48\x85\xf6\x74\x00\x5b\x89\xea\x4c\x89\xe7\x5d\x41\x5c",
            QuakeLiveFunction::SV_DropClient => b"\x41\x54\x55\x48\x89\xfd\x53\x48\x83\xec\x00\x83\x3f\x00\x0f\x84\x00\x00\x00\x00\x48\x8b\x87\x00\x00\x00\x00\x49\x89\xf4\x48\x85\xc0\x74\x00\xf6\x80\xe0\x01\x00\x00\x00\x75\x00\xbb\x00\x00\x00\x00",
            QuakeLiveFunction::Sys_SetModuleOffset => b"\x55\x48\x89\xf2\x31\xc0\x48\x89\xf5\x48\x89\xfe\x53\x48\x89\xfb\xbf\x00\x00\x00\x00\x48\x83\xec\x00\xe8\x00\x00\x00\x00\xbf\x00\x00\x00\x00\xb9\x00\x00\x00\x00\x48\x89\xde\xf3\xa6\x74\x00",
            QuakeLiveFunction::SV_SpawnServer => b"\x41\x55\x41\x54\x41\x89\xf4\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00\x31\xc0\xbf\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_ExecuteString => b"\x41\x54\x49\x89\xfc\x55\x53\xe8\x00\x00\x00\x00\x44\x8b\x0d\x00\x00\x00\x00\x45\x85\xc9\x0f\x84\x00\x00\x00\x00\x48\x8b\x1d\x00\x00\x00\x00\xbd\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x90",
        }
    }

    pub(crate) fn mask(&self) -> &[u8] {
        match self {
            QuakeLiveFunction::Com_Printf => b"XXXXXXX----XXXXXX----XXXX----XXXX----XXXX----",
            QuakeLiveFunction::Cmd_AddCommand => b"XXXXXXXXXXXXXXX-XXX----XXXX-X-XXXXXXXXX-XXX-XXX",
            QuakeLiveFunction::Cmd_Args => b"XX----XX----XXX-XX----XXXXX-XX-XXXX-XXXXXXX----X",
            QuakeLiveFunction::Cmd_Argv => b"XX----X----X-XXXXXXX----XX",
            QuakeLiveFunction::Cmd_Argc => b"XX----X",
            QuakeLiveFunction::Cmd_Tokenizestring => {
                b"XXXXXX--X----XXXXXX----XXXX----X----X----XX----"
            }
            QuakeLiveFunction::Cbuf_ExecuteText => {
                b"XX-X-XX-X-XXX-X----XXXXX----XXX-XXXX-XX-X-X----X"
            }
            QuakeLiveFunction::Cvar_FindVar => b"XXXXXXXX-X----XXXX----XXXX-X-XXXXXX-XXXX-XXXXXX",
            QuakeLiveFunction::Cvar_Get => b"XXXXXXXXXXXXXXXXXXXXXX----XXXXX----XXXX----XX",
            QuakeLiveFunction::Cvar_GetLimit => b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX-X----XXXXXXXXX",
            QuakeLiveFunction::Cvar_Set2 => b"XXXXXXXXXXXXXXXXXXXXXXXXXXX----XXXX-X----XXXX----",
            QuakeLiveFunction::SV_SendServerCommand => {
                b"XXXXXXXXXXXX----XXXXXX----XXXX----XXXX----XXXX----"
            }
            QuakeLiveFunction::SV_ExecuteClientCommand => {
                b"XXXXXXXXXXXXXXX----XXXX-X----XXX----XXXX-X----XX"
            }
            QuakeLiveFunction::SV_Shutdown => b"XXX----XXXX-XXX-XXXX-XX",
            QuakeLiveFunction::SV_Map_f => b"XXX----XXXXXXX----XXXXX----XXXX----XXX----X----XXX",
            QuakeLiveFunction::SV_ClientEnterWorld => {
                b"XXXXXX--------XXXXXXXX----XXXXXX----XXXXXX-X----"
            }
            QuakeLiveFunction::SV_SetConfigstring => {
                b"XXXXXXXXXXXXXXXX----XXXXX----XXXX----XXXX----XXXX-"
            }
            QuakeLiveFunction::SV_GetConfigstring => {
                b"XXXXXXXXXXXXXXX-XX----X-XXXX----XXXX-XXXXXXXXX"
            }
            QuakeLiveFunction::SV_DropClient => {
                b"XXXXXXXXXX-XX-XX----XXX----XXXXXXX-XXXXXX-X-X----"
            }
            QuakeLiveFunction::Sys_SetModuleOffset => {
                b"XXXXXXXXXXXXXXXXX----XXX-X----X----X----XXXXXX-"
            }
            QuakeLiveFunction::SV_SpawnServer => {
                b"XXXXXXXXXXXXXXX----XXXXX----XXXX----XXX----XXX----"
            }
            QuakeLiveFunction::Cmd_ExecuteString => {
                b"XXXXXXXX----XXX----XXXXX----XXX----X----XXXX-X-X"
            }
        }
    }
}
