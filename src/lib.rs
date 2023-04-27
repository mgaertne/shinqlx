macro_rules! debug_println {
    () => {
        #[cfg(debug_assertions)]
        dbg!("{}", DEBUG_PRINT_PREFIX)
    };
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        dbg!("{} {}", DEBUG_PRINT_PREFIX, $($arg)*)
    };
}

mod commands;
mod hooks;
mod quake_common;

use crate::commands::{
    cmd_center_print, cmd_py_rcon, cmd_regular_print, cmd_send_server_command, cmd_slap, cmd_slay,
};
#[cfg(debug_assertions)]
use crate::quake_common::DEBUG_PRINT_PREFIX;
use crate::quake_common::{cvar_t, AddCommand, FindCVar, QuakeLiveEngine};
use ctor::ctor;
use std::ffi::c_int;

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

extern "C" {
    static mut common_initialized: c_int;
    fn PyCommand();
    fn RestartPython();
    fn PyMinqlx_Initialize() -> PyMinqlx_InitStatus_t;
}

// Currently called by My_Cmd_AddCommand(), since it's called at a point where we
// can safely do whatever we do below. It'll segfault if we do it at the entry
// point, since functions like Cmd_AddCommand need initialization first.
fn initialize_static() {
    debug_println!("Initializing...");
    QuakeLiveEngine::add_command("cmd", cmd_send_server_command);
    QuakeLiveEngine::add_command("cp", cmd_center_print);
    QuakeLiveEngine::add_command("print", cmd_regular_print);
    QuakeLiveEngine::add_command("slap", cmd_slap);
    QuakeLiveEngine::add_command("slay", cmd_slay);
    QuakeLiveEngine::add_command("qlx", cmd_py_rcon);
    QuakeLiveEngine::add_command("pycmd", PyCommand);
    QuakeLiveEngine::add_command("pyrestart", RestartPython);
    let res = unsafe { PyMinqlx_Initialize() };
    if res != PyMinqlx_InitStatus_t::PYM_SUCCESS {
        debug_println!("Python initialization failed: {}", res);
        panic!("Python initialization failed.");
    }

    unsafe {
        common_initialized = 1;
    }
}

extern "C" {
    static mut sv_maxclients: *const cvar_t;
    static mut cvars_initialized: c_int;
}

// Called after the game is initialized.
fn initialize_cvars() {
    let Some(maxclients) = QuakeLiveEngine::find_cvar("sv_maxclients") else {
        return;
    };
    unsafe {
        sv_maxclients = maxclients.get_cvar();
        cvars_initialized = 1;
    }
}

extern "C" {
    fn EntryPoint();
}

#[ctor]
fn initialize() {
    unsafe { EntryPoint() };
}
