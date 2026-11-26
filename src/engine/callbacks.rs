//! What the engine calls back into. The composition root hands the table over when it creates the engine, so the
//! engine does not depend on the code that reacts to it.

use core::ffi::{c_char, c_int};

#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;

use super::{PythonInitializationError, prelude::*};

/// Replacements and notifications for the engine, grouped like its function tables.
pub(crate) struct EngineCallbacks {
    pub(crate) console: ConsoleCallbacks,
    pub(crate) cvar: CvarCallbacks,
    pub(crate) server: ServerCallbacks,
    pub(crate) module_loader: ModuleLoaderCallbacks,
    pub(crate) vm: VmCallbacks,
    pub(crate) game: GameCallbacks,
    pub(crate) entity: EntityCallbacks,
    pub(crate) item: ItemCallbacks,
    pub(crate) lookup: LookupCallbacks,
    /// The commands that are added to the server console, by name.
    pub(crate) console_commands: &'static [(&'static str, unsafe extern "C" fn())],
    /// Sets a configstring on behalf of the engine and lets plugins see it.
    pub(crate) set_configstring: fn(u32, &str),
    pub(crate) initialize_scripting: fn() -> Result<(), PythonInitializationError>,
}

/// Told what the engine found, so that others can patch or use it.
pub(crate) struct LookupCallbacks {
    /// Called with the memory mappings of the engine once its functions are found.
    #[cfg(target_os = "linux")]
    pub(crate) engine_functions_found: fn(&[&MemoryMap]),
    /// Called once the hooks of the game module are in place, with the address of its callvote
    /// command if that was looked up.
    pub(crate) game_module_hooked: fn(Option<usize>),
}

pub(crate) struct ConsoleCallbacks {
    pub(crate) cmd_addcommand: extern "C" fn(*const c_char, unsafe extern "C" fn()),
    pub(crate) com_printf: unsafe extern "C" fn(*const c_char, ...),
}

pub(crate) struct CvarCallbacks {
    pub(crate) cvar_set2: extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
}

pub(crate) struct ServerCallbacks {
    pub(crate) sv_executeclientcommand: extern "C" fn(*mut client_t, *const c_char, qboolean),
    pub(crate) sv_cliententerworld: extern "C" fn(*mut client_t, *mut usercmd_t),
    pub(crate) sv_sendservercommand: unsafe extern "C" fn(*mut client_t, *const c_char, ...),
    pub(crate) sv_setconfigstring: extern "C" fn(c_int, *const c_char),
    pub(crate) sv_dropclient: extern "C" fn(*mut client_t, *const c_char),
    pub(crate) sv_spawnserver: extern "C" fn(*mut c_char, qboolean),
    #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
    pub(crate) sv_handlesteampacket: extern "C" fn(*const netadr_t, *const msg_t) -> c_int,
}

pub(crate) struct ModuleLoaderCallbacks {
    pub(crate) sys_setmoduleoffset: extern "C" fn(*mut c_char, unsafe extern "C" fn()),
}

/// The entries of the call table of the game module.
pub(crate) struct VmCallbacks {
    pub(crate) g_initgame: extern "C" fn(c_int, c_int, c_int),
    pub(crate) g_runframe: extern "C" fn(c_int),
    pub(crate) g_shutdowngame: extern "C" fn(c_int),
}

pub(crate) struct GameCallbacks {
    pub(crate) client_connect: extern "C" fn(c_int, qboolean, qboolean) -> *const c_char,
    pub(crate) client_spawn: extern "C" fn(*mut gentity_t),
}

pub(crate) struct EntityCallbacks {
    pub(crate) g_startkamikaze: extern "C" fn(*mut gentity_t),
    #[allow(
        clippy::type_complexity,
        reason = "spells out the signature of the engine function"
    )]
    pub(crate) g_damage: extern "C" fn(
        *mut gentity_t,
        *mut gentity_t,
        *mut gentity_t,
        *mut vec3_t,
        *mut vec3_t,
        c_int,
        c_int,
        c_int,
    ),
    pub(crate) fire_weapon: extern "C" fn(*mut gentity_t),
    pub(crate) stats_medal: extern "C" fn(*mut gentity_t, *const c_char, c_int),
    pub(crate) stats_flag_status: extern "C" fn(c_int, c_int, *mut gentity_t),
}

pub(crate) struct ItemCallbacks {
    pub(crate) touch_item: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t),
}
