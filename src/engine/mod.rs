//! Everything that reaches into the dedicated server and its game module: finding functions and globals, hooking
//! them and calling them.

pub(crate) mod callbacks;
mod error;
mod instance;
#[cfg(test)]
mod mock;
#[allow(
    missing_docs,
    reason = "the variants of the module are the C symbol names of the engine functions"
)]
pub mod patterns;
pub(crate) mod prelude;
pub(crate) mod qagame;
pub(crate) mod qzeroded;
mod symbols;
#[cfg(test)]
mod test_helpers;
pub(crate) mod types;

pub use error::{PythonInitializationError, QuakeLiveEngineError};
pub(crate) use instance::QuakeLiveEngine;
#[cfg(test)]
pub(crate) use mock::{MockEngineBuilder, MockQuakeEngine, mock_quake_functions};
use qagame::module::VmFunctions;
pub(crate) use qagame::{entity::*, game::*, item::*, module::SetModuleOffset};
use qzeroded::{StaticDetours, StaticFunctions};
pub(crate) use qzeroded::{console::*, cvar::*, server::*};

#[cfg_attr(
    any(not(target_os = "linux"), test),
    allow(
        dead_code,
        reason = "stand-ins for engine symbols that not every test calls"
    )
)]
pub(crate) const QAGAME: &str = "qagamex64.so";
