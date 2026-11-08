#[cfg(any(
    feature = "clientkick-fix",
    all(feature = "fps-patch", target_os = "linux")
))]
use core::fmt;

#[cfg(feature = "clientkick-fix")]
pub(crate) mod clientkick_fix;
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
pub(crate) mod query_rate_limiter;
#[cfg(all(feature = "fps-patch", target_os = "linux"))]
pub(crate) mod sv_fps_patch;

/// Why a patch was not applied.
#[cfg(any(
    feature = "clientkick-fix",
    all(feature = "fps-patch", target_os = "linux")
))]
#[derive(Debug)]
pub(crate) enum PatchError {
    /// The memory protection could not be changed.
    Protect(region::Error),
    /// Original bytes, patch and mask differ in length.
    #[cfg(feature = "clientkick-fix")]
    LengthMismatch,
    /// The code at the target is neither the expected original nor the patched code.
    #[cfg(feature = "clientkick-fix")]
    UnexpectedCode,
    /// No entry of the cvar table of the game module has the expected layout.
    #[cfg(all(feature = "fps-patch", target_os = "linux"))]
    TableEntryNotFound,
    /// More than one entry of the cvar table of the game module has the expected layout.
    #[cfg(all(feature = "fps-patch", target_os = "linux"))]
    TableEntryAmbiguous,
}

#[cfg(any(
    feature = "clientkick-fix",
    all(feature = "fps-patch", target_os = "linux")
))]
impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protect(err) => write!(f, "{err}"),
            #[cfg(feature = "clientkick-fix")]
            Self::LengthMismatch => write!(f, "original bytes, patch and mask differ in length"),
            #[cfg(feature = "clientkick-fix")]
            Self::UnexpectedCode => write!(f, "the code at the target is not the expected one"),
            #[cfg(all(feature = "fps-patch", target_os = "linux"))]
            Self::TableEntryNotFound => write!(f, "no cvar table entry with the expected layout"),
            #[cfg(all(feature = "fps-patch", target_os = "linux"))]
            Self::TableEntryAmbiguous => {
                write!(f, "several cvar table entries with the expected layout")
            }
        }
    }
}

/// What a successful patch run did to the code.
#[cfg(any(
    feature = "clientkick-fix",
    all(feature = "fps-patch", target_os = "linux")
))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PatchOutcome {
    Applied,
    AlreadyApplied,
}
