//! ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from
//! [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work.
//! Support for Python 3.8 and above should work out of the box.

#![cfg_attr(not(test), no_main)]
#![feature(arbitrary_self_types, c_variadic, auto_traits, negative_impls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]

extern crate alloc;

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
    pub(crate) use log::{debug, error, warn};
}

use crate::prelude::*;
use once_cell::sync::OnceCell;

use ctor::ctor;
use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::{Config, Handle};
use parking_lot::RwLock;

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
pub(crate) const QZERODED: &str = "qzeroded.x64";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
pub(crate) const QZERODED: &str = "qzeroded.x86";

pub(crate) static MAIN_LOGGER: OnceCell<Handle> = OnceCell::new();
pub(crate) static MAIN_ENGINE: RwLock<Option<QuakeLiveEngine>> = RwLock::new(None);

fn initialize_logging() {
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{([{t}]):<32.32} {({l}:):<6.6} {m}{n}",
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

#[ctor]
fn initialize() {
    if let Some(progname) = std::env::args().next() {
        if !progname.ends_with(QZERODED) {
            return;
        }
    } else {
        return;
    }

    initialize_logging();
    let main_engine = QuakeLiveEngine::new();
    if let Err(err) = main_engine.search_static_functions() {
        error!("{:?}", err);
        error!("Static functions could not be initializied. Exiting.");
        panic!("Static functions could not be initializied. Exiting.");
    }

    debug!("Shared library loaded");
    if let Err(err) = main_engine.hook_static() {
        error!("{:?}", err);
        error!("Failed to hook static methods. Exiting.");
        panic!("Failed to hook static methods. Exiting.");
    }

    let mut guard = MAIN_ENGINE.write();
    *guard = Some(main_engine);
}
