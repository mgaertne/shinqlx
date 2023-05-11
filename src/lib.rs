#![feature(arbitrary_self_types)]
extern crate alloc;
macro_rules! debug_println {
    () => {
        #[cfg(debug_assertions)]
        println!("{}", DEBUG_PRINT_PREFIX)
    };
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!("{} {}", DEBUG_PRINT_PREFIX, $($arg)*)
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
#[cfg(debug_assertions)]
use crate::quake_common::DEBUG_PRINT_PREFIX;
use crate::quake_common::{client_t, AddCommand, FindCVar, QuakeLiveEngine};
use crate::PyMinqlx_InitStatus_t::PYM_SUCCESS;
use ctor::ctor;

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
    QuakeLiveEngine::add_command("cmd", cmd_send_server_command);
    QuakeLiveEngine::add_command("cp", cmd_center_print);
    QuakeLiveEngine::add_command("print", cmd_regular_print);
    QuakeLiveEngine::add_command("slap", cmd_slap);
    QuakeLiveEngine::add_command("slay", cmd_slay);
    QuakeLiveEngine::add_command("qlx", cmd_py_rcon);
    QuakeLiveEngine::add_command("pycmd", cmd_py_command);
    QuakeLiveEngine::add_command("pyrestart", cmd_restart_python);

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
    let Some(maxclients) = QuakeLiveEngine::find_cvar("sv_maxclients") else {
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
    fn EntryPoint();
}

#[ctor]
fn initialize() {
    dbg!(std::mem::size_of::<client_t>());
    unsafe { EntryPoint() };
}
