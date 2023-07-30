#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]

#[cfg(test)]
#[cfg(not(miri))]
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

use crate::quake_live_engine::QuakeLiveEngine;
use core::sync::atomic::AtomicI32;
use ctor::ctor;
use parking_lot::RwLock;

pub(crate) const DEBUG_PRINT_PREFIX: &str = "[shinqlx]";

pub(crate) const SV_TAGS_PREFIX: &str = "shinqlx";

pub(crate) static ALLOW_FREE_CLIENT: AtomicI32 = AtomicI32::new(-1);

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QZERODED: &str = "qzeroded.x64";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QZERODED: &str = "qzeroded.x86";

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QAGAME: &str = "qagamex64.so";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QAGAME: &str = "qagamei386.so";

pub(crate) static MAIN_ENGINE: RwLock<Option<QuakeLiveEngine>> = RwLock::new(None);

#[ctor]
fn initialize() {
    if let Some(progname) = std::env::args().next() {
        if !progname.ends_with(QZERODED) {
            return;
        }
    } else {
        return;
    }

    let main_engine = QuakeLiveEngine::new();
    if let Err(err) = main_engine.search_static_functions() {
        debug_println!(format!("{:?}", err));
        debug_println!("Static functions could not be initializied. Exiting.");
        panic!("Static functions could not be initializied. Exiting.");
    }

    debug_println!("Shared library loaded");
    if let Err(err) = main_engine.hook_static() {
        debug_println!(format!("{:?}", err));
        debug_println!("Failed to hook static methods. Exiting.");
        panic!("Failed to hook static methods. Exiting.");
    }

    let mut guard = MAIN_ENGINE.write();
    *guard = Some(main_engine);
}
