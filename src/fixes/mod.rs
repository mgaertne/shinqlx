#[cfg(feature = "clientkick-fix")]
use core::fmt;

#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;

#[cfg(feature = "clientkick-fix")]
pub(crate) mod clientkick_fix;
#[cfg(unix)]
pub(crate) mod crash_guard;
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
pub(crate) mod query_rate_limiter;
#[cfg(all(target_os = "linux", feature = "race-hooks"))]
pub(crate) mod race_touch;
pub(crate) mod reliable_guard;
pub(crate) mod scoreboard_trim;
#[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
pub(crate) mod sv_fps_patch;
#[cfg(feature = "zmq-rcon-fix")]
pub(crate) mod zmq_rcon;

/// Looks up what the fixes need in the code of the engine.
#[cfg(target_os = "linux")]
pub(crate) fn engine_functions_found(maps: &[&MemoryMap]) {
    #[cfg(feature = "zmq-rcon-fix")]
    zmq_rcon::register_socket_of(maps);
    #[cfg(not(feature = "zmq-rcon-fix"))]
    let _ = maps;
}

/// Patches the game module once its hooks are in place.
pub(crate) fn game_module_hooked(callvote_command: Option<usize>) {
    #[cfg(feature = "clientkick-fix")]
    if let Some(address) = callvote_command {
        clientkick_fix::patch_callvote_command(address);
    }
    #[cfg(not(feature = "clientkick-fix"))]
    let _ = callvote_command;
}

/// Why a patch was not applied.
#[cfg(feature = "clientkick-fix")]
#[derive(Debug)]
pub(crate) enum PatchError {
    /// The memory protection could not be changed.
    Protect(region::Error),
    /// Original bytes, patch and mask differ in length.
    LengthMismatch,
    /// The code at the target is neither the expected original nor the patched code.
    UnexpectedCode,
}

#[cfg(feature = "clientkick-fix")]
impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protect(err) => write!(f, "{err}"),
            Self::LengthMismatch => write!(f, "original bytes, patch and mask differ in length"),
            Self::UnexpectedCode => write!(f, "the code at the target is not the expected one"),
        }
    }
}

/// What a successful patch run did to the code.
#[cfg(feature = "clientkick-fix")]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PatchOutcome {
    Applied,
    AlreadyApplied,
}
