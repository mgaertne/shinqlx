//! # ShiNQLX
//!
//! A Rust implementation of the [minqlx](https://github.com/MinoMino/minqlx)
//! Quake Live server modification.
//!
//! This library provides hooks into the Quake Live dedicated server,
//! allowing for extensive modification and extension of the game.
//!
//! ## Features
//!
//! * Python scripting interface for game extensions
//! * Event-based plugin system
//! * Game state manipulation
//! * Player management and permissions
//!
//! ## Usage
//!
//! This library is loaded by the Quake Live dedicated server at startup
//! and provides a Python API for interacting with the game.
//!
//! ## Cargo features
//!
//! * `alloc` (default): uses jemalloc, on Windows mimalloc, as global allocator.
//! * `steam-query-limit` (default): rate limits server queries (`A2S_INFO`, `A2S_PLAYER`, `A2S_RULES`) per source
//!   address and overall, see `DDOS-PROTECTION.md` in the repository. Configured through the cvars
//!   `qlx_steamQueryRate`, `qlx_steamQueryBurst` and `qlx_steamQueryGlobalRate`; the rules query has its own limits
//!   in `qlx_steamQueryRulesRate`, `qlx_steamQueryRulesBurst` and `qlx_steamQueryRulesGlobalRate`, and
//!   `qlx_steamQueryDropRules` drops it.
//! * `patches`: enables `clientkick-fix`, `fps-patch`, `rcon-disconnect-fix` and `steam-query-limit`. Disabled by
//!   default as a group, the last two are part of the default features on their own.
//! * `rcon-disconnect-fix` (default): drops the engine's report `zmq RCON client disconnected: ...` before it reaches
//!   `Com_Printf`. The engine erases dead ZMQ RCON clients in a loop that reports each deletion through `Com_Printf`,
//!   which enters the loop again; two or more dead clients at once leave the outer loop with a freed entry and crash
//!   the server. Plugins no longer see the report in `console_print`.
//! * `clientkick-fix`: patches the callvote clientkick lookup in `qagame`. Experimental, disabled by
//!   default.
//! * `fps-patch`: makes `qagame` register `sv_fps` with the default 125 instead of the read-only 40.
//!   Experimental, disabled by default.
//! * `bench-support`: exposes internals to the benchmarks. Not meant for production builds.

#![cfg_attr(not(test), no_main)]
#![allow(
    stable_features,
    reason = "the feature gates stay for toolchains where they are not yet stable"
)]
#![feature(arbitrary_self_types, c_variadic)]
#![cfg_attr(test, feature(stmt_expr_attributes))]

extern crate alloc;
extern crate core;

#[cfg(all(feature = "alloc", not(miri)))]
cfg_if::cfg_if! {
    if #[cfg(not(windows))] {
        #[global_allocator]
        static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
    } else {
        #[global_allocator]
        static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
    }
}

mod commands;
mod ffi;
mod fixes;
mod hooks;
mod logging;
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub use ffi::python::bench_support;
mod quake_live_engine;
#[allow(
    missing_docs,
    reason = "the variants of the module are the C symbol names of the engine functions"
)]
pub mod quake_live_functions;

pub(crate) mod prelude {
    pub(crate) use alloc::format;
    pub(crate) use core::{mem, ptr};

    pub(crate) use log::{debug, error, warn};
    #[cfg(test)]
    pub(crate) use serial_test::serial;

    #[cfg(not(test))]
    pub(crate) use crate::quake_live_engine::QuakeLiveEngine;
    pub(crate) use crate::quake_live_engine::QuakeLiveEngineError;
    #[cfg(test)]
    pub(crate) use crate::quake_live_engine::{
        MockEngineBuilder, MockQuakeEngine as QuakeLiveEngine,
    };
}

use core::hint::cold_path;
use std::{env, sync::LazyLock};

use arc_swap::ArcSwapOption;
use chrono::{DateTime, Utc};
#[cfg(not(test))]
use ctor::ctor;
use signal_hook::consts::SIGSEGV;
use tap::TapFallible;

use crate::prelude::*;

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) const QZERODED: &str = "qzeroded.x64";

pub(crate) static MAIN_ENGINE: LazyLock<ArcSwapOption<QuakeLiveEngine>> =
    LazyLock::new(ArcSwapOption::empty);

pub(crate) static _INIT_TIME: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);

#[cfg_attr(not(test), ctor(unsafe))]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
fn initialize() {
    if env::args()
        .next()
        .filter(|progname| progname.ends_with(QZERODED))
        .is_none()
    {
        cold_path();
        return;
    }

    // SAFETY: the handler only calls exit, which is safe in a signal handler.
    let _ = unsafe {
        signal_hook_registry::register_signal_unchecked(SIGSEGV, move || {
            signal_hook::low_level::exit(1);
        })
    }
    .tap_err(|err| {
        error!(target: "shinqlx", func = "initialize"; "{err:?}");
        error!(target: "shinqlx", func = "initialize"; "Could not register exit handler");
        panic!("Could not register exit handler");
    });

    logging::initialize_logging();
    let main_engine = QuakeLiveEngine::new();
    let _ = main_engine.search_static_functions().tap_err(|err| {
        error!(target: "shinqlx", func = "initialize"; "{err:?}");
        error!(target: "shinqlx", func = "initialize"; "Static functions could not be initializied. Exiting.");
        panic!("Static functions could not be initializied. Exiting.");
    });

    debug!(target: "shinqlx", func = "initialize"; "Shared library loaded");
    let _ = main_engine.hook_static().tap_err(|err| {
        error!(target: "shinqlx", func = "initialize"; "{err:?}");
        error!(target: "shinqlx", func = "initialize"; "Failed to hook static methods. Exiting.");
        panic!("Failed to hook static methods. Exiting.");
    });

    MAIN_ENGINE.store(Some(main_engine.into()));

    let _ = _INIT_TIME.timestamp();
}

#[cfg(test)]
mod lib_tests {
    use super::*;
    use crate::prelude::serial;

    #[test]
    #[serial]
    fn test_initialize_non_qzeroded_program() {
        // SAFETY: the tests changing the environment run serially.
        unsafe {
            env::set_var("RUST_TEST_PROGRAM", "some_other_program");
        }

        initialize();

        assert!(MAIN_ENGINE.load().is_none());
    }
}
