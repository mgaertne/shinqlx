#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types)]
#![feature(c_variadic)]
#[cfg(test)]
#[macro_use]
extern crate hamcrest;

macro_rules! debug_println {
    () => {
        println!("{}", crate::quake_common::DEBUG_PRINT_PREFIX)
    };
    ($($arg:tt)*) => {
        println!("{} {}", crate::quake_common::DEBUG_PRINT_PREFIX, $($arg)*)
    };
}

mod commands;
mod hooks;
mod pyminqlx;
mod quake_common;

use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
#[cfg(not(feature = "cembed"))]
use crate::pyminqlx::pyminqlx_initialize;
#[cfg(feature = "cembed")]
use crate::quake_common::cvar_t;
use crate::quake_common::{AddCommand, FindCVar, QuakeLiveEngine};
use crate::PyMinqlx_InitStatus_t::PYM_SUCCESS;
use ctor::ctor;
use std::env::args;

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

#[cfg(feature = "cembed")]
extern "C" {
    fn PyMinqlx_Initialize() -> PyMinqlx_InitStatus_t;
}

// Currently called by My_Cmd_AddCommand(), since it's called at a point where we
// can safely do whatever we do below. It'll segfault if we do it at the entry
// point, since functions like Cmd_AddCommand need initialization first.
fn initialize_static() {
    debug_println!("Initializing...");
    QuakeLiveEngine::default().add_command("cmd", cmd_send_server_command);
    QuakeLiveEngine::default().add_command("cp", cmd_center_print);
    QuakeLiveEngine::default().add_command("print", cmd_regular_print);
    QuakeLiveEngine::default().add_command("slap", cmd_slap);
    QuakeLiveEngine::default().add_command("slay", cmd_slay);
    QuakeLiveEngine::default().add_command("qlx", cmd_py_rcon);
    QuakeLiveEngine::default().add_command("pycmd", cmd_py_command);
    QuakeLiveEngine::default().add_command("pyrestart", cmd_restart_python);

    #[cfg(feature = "cembed")]
    let res = unsafe { PyMinqlx_Initialize() };
    #[cfg(not(feature = "cembed"))]
    let res = pyminqlx_initialize();
    if res != PYM_SUCCESS {
        debug_println!("Python initialization failed.");
        panic!("Python initialization failed.");
    }

    unsafe { COMMON_INITIALIZED = true };
}

#[cfg(feature = "cembed")]
extern "C" {
    static mut sv_maxclients: *const cvar_t;
}

// Called after the game is initialized.
fn initialize_cvars() {
    let Some(maxclients) = QuakeLiveEngine::default().find_cvar("sv_maxclients") else {
        return;
    };
    #[cfg(feature = "cembed")]
    unsafe {
        sv_maxclients = maxclients.get_cvar();
    }
    unsafe { SV_MAXCLIENTS = maxclients.get_integer() };
    unsafe { CVARS_INITIALIZED = true };
}

extern "C" {
    fn SearchFunctions();
    fn InitializeStatic();
    fn HookStatic();
}

#[cfg(target_pointer_width = "64")]
const QZERODED: &str = "qzeroded.x64";
#[cfg(target_pointer_width = "32")]
const QZERODED: &str = "qzeroded.x86";

#[ctor]
fn initialize() {
    let progname = args().next().unwrap();
    if !progname.ends_with(QZERODED) {
        return;
    }

    unsafe { SearchFunctions() };

    unsafe { InitializeStatic() };

    debug_println!("Shared library loaded");
    unsafe { HookStatic() };
}
