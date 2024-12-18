//! ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from
//! [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work.
//! Support for Python 3.8 and above should work out of the box.

#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]

extern crate alloc;

#[cfg(all(feature = "alloc", not(miri)))]
cfg_if::cfg_if! {
    if #[cfg(not(target_os = "windows"))] {
        #[global_allocator]
        static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    } else {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    }
}

mod commands;
mod ffi;
mod hooks;
#[cfg(feature = "patches")]
mod patches;
mod quake_live_engine;
mod quake_live_functions;

pub(crate) mod prelude {
    #[cfg(not(test))]
    pub(crate) use crate::quake_live_engine::QuakeLiveEngine;
    pub(crate) use crate::quake_live_engine::QuakeLiveEngineError;
    #[cfg(test)]
    pub(crate) use crate::quake_live_engine::{
        MockEngineBuilder, MockQuakeEngine as QuakeLiveEngine,
    };

    pub(crate) use alloc::format;
    pub(crate) use core::{mem, ptr};
    pub(crate) use log::{debug, error, warn};
    #[cfg(test)]
    pub(crate) use serial_test::serial;
}

use crate::prelude::*;
use std::path::Path;

use arc_swap::ArcSwapOption;
#[cfg(not(test))]
use ctor::ctor;
use log::LevelFilter;
use log4rs::{
    Config,
    append::console::ConsoleAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
};
use once_cell::sync::Lazy;
use signal_hook::consts::SIGSEGV;

#[cfg_attr(test, allow(dead_code))]
pub(crate) const QZERODED: &str = "qzeroded.x64";

pub(crate) static MAIN_ENGINE: Lazy<ArcSwapOption<QuakeLiveEngine>> =
    Lazy::new(ArcSwapOption::empty);

fn initialize_logging() {
    if Path::new("./shinqlx_log.yml").exists() {
        log4rs::config::init_file("shinqlx_log.yml", Default::default()).unwrap();
    } else {
        let stdout = ConsoleAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                "{([{t}]):<9.9} {({l}:):<6.6} {m}{n}",
            )))
            .build();

        let level_filter = LevelFilter::Info;

        let config = Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout)))
            .build(Root::builder().appender("stdout").build(level_filter))
            .unwrap();

        log4rs::init_config(config).unwrap();
    }
}

#[cfg_attr(not(test), ctor)]
#[cfg_attr(test, allow(dead_code))]
fn initialize() {
    if std::env::args()
        .next()
        .filter(|progname| progname.ends_with(QZERODED))
        .is_none()
    {
        return;
    }

    if let Err(err) = unsafe {
        signal_hook_registry::register_signal_unchecked(SIGSEGV, move || {
            signal_hook::low_level::exit(1);
        })
    } {
        error!(target: "shinqlx", "{:?}", err);
        error!(target: "shinqlx", "Could not register exit handler");
        panic!("Could not register exit handler");
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
