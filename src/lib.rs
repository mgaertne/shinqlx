//! ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from
//! [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work.
//! Support for Python 3.8 and above should work out of the box.

#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

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
    #[cfg(test)]
    pub(crate) use crate::quake_live_engine::MockQuakeEngine as QuakeLiveEngine;
    #[cfg(test)]
    pub(crate) use crate::quake_live_engine::MockQuakeEngine;
    #[cfg(not(test))]
    pub(crate) use crate::quake_live_engine::QuakeLiveEngine;
    pub(crate) use crate::quake_live_engine::QuakeLiveEngineError;

    pub(crate) use alloc::format;
    pub(crate) use core::{mem, ptr};
    pub(crate) use log::{debug, error, warn};
    #[cfg(test)]
    pub(crate) use serial_test::serial;
}

use crate::prelude::*;

use alloc::sync::Arc;
use arc_swap::ArcSwapOption;
#[cfg(not(test))]
use ctor::ctor;
use log::LevelFilter;
use log4rs::{
    append::console::ConsoleAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    Config, Handle,
};
use once_cell::sync::{Lazy, OnceCell};
use signal_hook::consts::SIGSEGV;

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QZERODED: &str = "qzeroded.x64";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QZERODED: &str = "qzeroded.x86";

pub(crate) static MAIN_LOGGER: OnceCell<Handle> = OnceCell::new();
pub(crate) static MAIN_ENGINE: Lazy<Arc<ArcSwapOption<QuakeLiveEngine>>> =
    Lazy::new(|| ArcSwapOption::empty().into());

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
