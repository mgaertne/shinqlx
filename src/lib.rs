//! ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from
//! [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work.
//! Support for Python 3.8 and above should work out of the box.

#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

extern crate alloc;

macro_rules! debug_println {
    () => {
        libc_println!("{}", "[shinqlx]")
    };
    ($($arg:tt)*) => {
        libc_println!("{} {}", "[shinqlx]", $($arg)*)
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

pub(crate) mod prelude {
    pub(crate) use crate::quake_live_engine::{QuakeLiveEngine, QuakeLiveEngineError};
    pub(crate) use crate::quake_types::*;
    pub(crate) use alloc::format;
    pub(crate) use libc_print::{libc_dbg, libc_println};
}

use crate::prelude::*;

use ctor::ctor;
use parking_lot::RwLock;

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QZERODED: &str = "qzeroded.x64";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QZERODED: &str = "qzeroded.x86";

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
