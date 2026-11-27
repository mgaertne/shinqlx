//! The engine and the game module both register `sv_fps` read-only with the default 40. A value
//! set on the command line makes the registration of the engine replace whatever is there, so the
//! default is changed in both.

use core::ffi::{CStr, c_int};

use crate::engine::prelude::cvar_flags;

const STOCK_SV_FPS: &CStr = c"40";
const PATCHED_SV_FPS: &CStr = c"125";

/// The default to register a cvar with instead of `default`: 125 where `sv_fps` would be
/// registered read-only with 40, `None` for every other registration.
pub(crate) fn replaced_default(name: &CStr, default: &CStr, flags: c_int) -> Option<&'static CStr> {
    (name.to_bytes().eq_ignore_ascii_case(b"sv_fps")
        && default == STOCK_SV_FPS
        && flags & cvar_flags::CVAR_ROM as c_int != 0)
        .then_some(PATCHED_SV_FPS)
}

#[cfg(test)]
mod sv_fps_patch_tests {
    use core::ffi::{CStr, c_int};

    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::replaced_default;
    use crate::engine::prelude::cvar_flags;

    const READ_ONLY: c_int = cvar_flags::CVAR_ROM as c_int;

    #[rstest]
    #[case(c"sv_fps", READ_ONLY)]
    #[case(c"SV_FPS", READ_ONLY | cvar_flags::CVAR_ARCHIVE as c_int)]
    fn the_read_only_default_of_sv_fps_is_replaced(#[case] name: &CStr, #[case] flags: c_int) {
        assert_eq!(replaced_default(name, c"40", flags), Some(c"125"));
    }

    #[rstest]
    #[case::another_cvar(c"sv_maxclients", c"40", READ_ONLY)]
    #[case::a_longer_name(c"sv_fpsx", c"40", READ_ONLY)]
    #[case::another_default(c"sv_fps", c"30", READ_ONLY)]
    #[case::a_value_of_the_user(c"sv_fps", c"40", 0)]
    #[case::a_lookup(c"sv_fps", c"", 0)]
    fn other_registrations_are_left_alone(
        #[case] name: &CStr,
        #[case] default: &CStr,
        #[case] flags: c_int,
    ) {
        assert_eq!(replaced_default(name, default, flags), None);
    }
}
