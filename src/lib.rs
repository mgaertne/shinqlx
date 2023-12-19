//! ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from
//! [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work.
//! Support for Python 3.8 and above should work out of the box.

#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

extern crate alloc;

mod commands;
mod ffi;
mod hooks;
mod patches;
mod quake_live_engine;
mod quake_live_functions;

pub(crate) mod prelude {
    pub(crate) use crate::ffi::c::quake_types::*;
    pub(crate) use crate::ffi::c::{Activator, Client, GameClient, GameEntity, GameItem};
    #[cfg(test)]
    pub(crate) use crate::quake_live_engine::MockQuakeEngine as QuakeLiveEngine;
    #[cfg(not(test))]
    pub(crate) use crate::quake_live_engine::QuakeLiveEngine;
    pub(crate) use crate::quake_live_engine::QuakeLiveEngineError;
    pub(crate) use alloc::format;
    pub(crate) use core::mem;
    pub(crate) use core::ptr;
    pub(crate) use log::{debug, error, warn};
    #[cfg(test)]
    pub(crate) use serial_test::serial;
}

use crate::prelude::*;
#[cfg(not(test))]
use ctor::ctor;
use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::{Config, Handle};
use once_cell::sync::{Lazy, OnceCell};
use signal_hook::consts::SIGSEGV;
use swap_arc::SwapArcOption;

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QZERODED: &str = "qzeroded.x64";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QZERODED: &str = "qzeroded.x86";

pub(crate) static MAIN_LOGGER: OnceCell<Handle> = OnceCell::new();
pub(crate) static MAIN_ENGINE: Lazy<SwapArcOption<QuakeLiveEngine>> =
    Lazy::new(|| SwapArcOption::new(None));

fn initialize_logging() {
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{([{t}]):<9.9} {({l}:):<6.6} {m}{n}",
        )))
        .build();

    #[cfg(debug_assertions)]
    let level_filter = LevelFilter::Debug;
    #[cfg(not(debug_assertions))]
    let level_filter = LevelFilter::Info;

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(level_filter))
        .unwrap();

    MAIN_LOGGER
        .set(log4rs::init_config(config).unwrap())
        .unwrap();
}

#[cfg_attr(not(test), ctor)]
#[cfg_attr(test, allow(dead_code))]
fn initialize() {
    let Some(progname) = std::env::args().next() else {
        return;
    };

    if !progname.ends_with(QZERODED) {
        return;
    }

    unsafe {
        signal_hook_registry::register_signal_unchecked(SIGSEGV, move || {
            signal_hook::low_level::exit(1);
        })
        .unwrap()
    };

    initialize_logging();
    let main_engine = QuakeLiveEngine::new();
    if let Err(err) = main_engine.search_static_functions() {
        error!(target: "shinqlx", "{:?}", err);
        error!(target: "shinqlx", "Static functions could not be initializied. Exiting.");
        panic!("Static functions could not be initializied. Exiting.");
    }

    debug!(target: "shinqlx", "Shared library loaded");
    if let Err(err) = main_engine.hook_static() {
        error!(target: "shinqlx", "{:?}", err);
        error!(target: "shinqlx", "Failed to hook static methods. Exiting.");
        panic!("Failed to hook static methods. Exiting.");
    }

    MAIN_ENGINE.store(Some(main_engine.into()));
}
