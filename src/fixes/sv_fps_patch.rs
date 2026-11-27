//! The engine and the game module both register `sv_fps` read-only with the default 40. Both
//! registrations are changed to a latched cvar with the default 125, so that a new rate takes
//! effect with the next map. The engine does not check the value, 0 even crashes it, so only rates
//! within the limits are let through.

use alloc::string::String;
use core::ffi::{CStr, c_int};

use crate::engine::prelude::cvar_flags;

const STOCK_SV_FPS: &CStr = c"40";
pub(crate) const DEFAULT_SV_FPS: &CStr = c"125";
const MINIMUM_SV_FPS: u32 = 10;
const MAXIMUM_SV_FPS: u32 = 250;
const MILLISECONDS_PER_SECOND: u32 = 1000;

fn is_sv_fps(name: &CStr) -> bool {
    name.to_bytes().eq_ignore_ascii_case(b"sv_fps")
}

/// The default and the flags to register a cvar with instead, where `sv_fps` would be registered
/// read-only with 40. `None` for every other registration.
pub(crate) fn replaced_registration(
    name: &CStr,
    default: &CStr,
    flags: c_int,
) -> Option<(&'static CStr, c_int)> {
    let read_only = cvar_flags::CVAR_ROM as c_int;
    (is_sv_fps(name) && default == STOCK_SV_FPS && flags & read_only != 0).then_some((
        DEFAULT_SV_FPS,
        flags & !read_only | cvar_flags::CVAR_LATCH as c_int,
    ))
}

fn rate_of(value: &CStr) -> Option<u32> {
    value
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|rate| (MINIMUM_SV_FPS..=MAXIMUM_SV_FPS).contains(rate))
}

/// Why a cvar must not get this value, `None` if it may.
pub(crate) fn rejection(name: &CStr, value: &CStr) -> Option<String> {
    (is_sv_fps(name) && rate_of(value).is_none()).then(|| {
        format!(
            "sv_fps has to be a whole number from {MINIMUM_SV_FPS} to {MAXIMUM_SV_FPS}, \"{}\" is \
             rejected.\n",
            value.to_string_lossy()
        )
    })
}

/// How the server runs with this rate: it counts in whole milliseconds for each frame.
pub(crate) fn effective_rate(value: &str) -> Option<String> {
    let rate: u32 = value.trim().parse().ok().filter(|&rate| rate > 0)?;
    let frame_time = (MILLISECONDS_PER_SECOND / rate).max(1);
    let frames = f64::from(MILLISECONDS_PER_SECOND) / f64::from(frame_time);
    Some(format!(
        "sv_fps {rate}: frames of {frame_time} ms, {frames:.1} per second"
    ))
}

#[cfg(test)]
mod sv_fps_patch_tests {
    use core::ffi::{CStr, c_int};

    use pretty_assertions::assert_eq;
    use rstest::*;

    use super::{effective_rate, rejection, replaced_registration};
    use crate::engine::prelude::cvar_flags;

    const READ_ONLY: c_int = cvar_flags::CVAR_ROM as c_int;
    const LATCH: c_int = cvar_flags::CVAR_LATCH as c_int;
    const ARCHIVE: c_int = cvar_flags::CVAR_ARCHIVE as c_int;

    #[rstest]
    #[case(c"sv_fps", READ_ONLY, LATCH)]
    #[case(c"SV_FPS", READ_ONLY | ARCHIVE, LATCH | ARCHIVE)]
    fn the_read_only_sv_fps_becomes_latched_with_125(
        #[case] name: &CStr,
        #[case] flags: c_int,
        #[case] expected: c_int,
    ) {
        assert_eq!(
            replaced_registration(name, c"40", flags),
            Some((c"125", expected))
        );
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
        assert_eq!(replaced_registration(name, default, flags), None);
    }

    #[rstest]
    #[case(c"10")]
    #[case(c"40")]
    #[case(c" 125 ")]
    #[case(c"250")]
    fn rates_within_the_limits_are_accepted(#[case] value: &CStr) {
        assert_eq!(rejection(c"sv_fps", value), None);
    }

    #[rstest]
    #[case(c"0")]
    #[case(c"9")]
    #[case(c"251")]
    #[case(c"-40")]
    #[case(c"62.5")]
    #[case(c"100fps")]
    #[case(c"fast")]
    #[case(c"")]
    #[case(c"99999999999")]
    fn other_values_are_rejected(#[case] value: &CStr) {
        assert!(rejection(c"Sv_Fps", value).is_some_and(|message| message.contains("10 to 250")));
    }

    #[test]
    fn other_cvars_take_any_value() {
        assert_eq!(rejection(c"sv_maxclients", c"0"), None);
    }

    #[rstest]
    #[case("40", "sv_fps 40: frames of 25 ms, 40.0 per second")]
    #[case("80", "sv_fps 80: frames of 12 ms, 83.3 per second")]
    #[case("250", "sv_fps 250: frames of 4 ms, 250.0 per second")]
    fn the_effective_rate_counts_in_whole_milliseconds(
        #[case] value: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(effective_rate(value).as_deref(), Some(expected));
    }

    #[rstest]
    #[case("0")]
    #[case("")]
    fn without_a_rate_there_is_no_effective_one(#[case] value: &str) {
        assert_eq!(effective_rate(value), None);
    }
}
