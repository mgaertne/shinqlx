#![allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    reason = "the variants mirror the names of the C constants of the engine"
)]

use pyo3::class::basic::CompareOp;

use super::{game_enums::richcmp_with_index, prelude::*};
use crate::ffi::c::prelude::*;

/// The flags of a cvar, e.g. for :func:`shinqlx.set_cvar`.
///
/// Members can be combined with ``|`` and checked with ``&``; the result is a plain integer.
/// The old constants ``shinqlx.CVAR_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "CvarFlags", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum CvarFlag {
    ARCHIVE = cvar_flags::CVAR_ARCHIVE as i32,
    USERINFO = cvar_flags::CVAR_USERINFO as i32,
    SERVERINFO = cvar_flags::CVAR_SERVERINFO as i32,
    SYSTEMINFO = cvar_flags::CVAR_SYSTEMINFO as i32,
    INIT = cvar_flags::CVAR_INIT as i32,
    LATCH = cvar_flags::CVAR_LATCH as i32,
    ROM = cvar_flags::CVAR_ROM as i32,
    USER_CREATED = cvar_flags::CVAR_USER_CREATED as i32,
    TEMP = cvar_flags::CVAR_TEMP as i32,
    CHEAT = cvar_flags::CVAR_CHEAT as i32,
    NORESTART = cvar_flags::CVAR_NORESTART as i32,
}

#[pymethods]
impl CvarFlag {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let same = other.cast::<Self>().ok().map(|same| *same.get() as i32);
        richcmp_with_index(slf.py(), *slf.get() as i32, same, other, op)
    }

    fn __or__(&self, other: i32) -> i32 {
        *self as i32 | other
    }

    fn __ror__(&self, other: i32) -> i32 {
        other | *self as i32
    }

    fn __and__(&self, other: i32) -> i32 {
        *self as i32 & other
    }

    fn __rand__(&self, other: i32) -> i32 {
        other & *self as i32
    }
}

/// The flags of the damage that a player takes, as passed to the ``damage`` event.
///
/// Members can be combined with ``|`` and checked with ``&``; the result is a plain integer.
/// The old constants ``shinqlx.DAMAGE_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "DamageFlags", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum DamageFlag {
    RADIUS = DAMAGE_RADIUS as i32,
    NO_ARMOR = DAMAGE_NO_ARMOR as i32,
    NO_KNOCKBACK = DAMAGE_NO_KNOCKBACK as i32,
    NO_PROTECTION = DAMAGE_NO_PROTECTION as i32,
    NO_TEAM_PROTECTION = DAMAGE_NO_TEAM_PROTECTION as i32,
}

#[pymethods]
impl DamageFlag {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let same = other.cast::<Self>().ok().map(|same| *same.get() as i32);
        richcmp_with_index(slf.py(), *slf.get() as i32, same, other, op)
    }

    fn __or__(&self, other: i32) -> i32 {
        *self as i32 | other
    }

    fn __ror__(&self, other: i32) -> i32 {
        other | *self as i32
    }

    fn __and__(&self, other: i32) -> i32 {
        *self as i32 & other
    }

    fn __rand__(&self, other: i32) -> i32 {
        other & *self as i32
    }
}

#[cfg(test)]
mod flag_enums_tests {
    use rstest::rstest;

    use super::{CvarFlag, DamageFlag};
    use crate::{ffi::python::prelude::*, prelude::*};

    #[rstest]
    #[case(CvarFlag::ARCHIVE, 1)]
    #[case(CvarFlag::USERINFO, 2)]
    #[case(CvarFlag::SERVERINFO, 4)]
    #[case(CvarFlag::SYSTEMINFO, 8)]
    #[case(CvarFlag::INIT, 16)]
    #[case(CvarFlag::LATCH, 32)]
    #[case(CvarFlag::ROM, 64)]
    #[case(CvarFlag::USER_CREATED, 128)]
    #[case(CvarFlag::TEMP, 256)]
    #[case(CvarFlag::CHEAT, 512)]
    #[case(CvarFlag::NORESTART, 1024)]
    fn cvar_flag_is_the_flag_of_the_engine(#[case] flag: CvarFlag, #[case] value: i32) {
        assert_eq!(flag.__index__(), value);
    }

    #[rstest]
    #[case(DamageFlag::RADIUS, 1)]
    #[case(DamageFlag::NO_ARMOR, 2)]
    #[case(DamageFlag::NO_KNOCKBACK, 4)]
    #[case(DamageFlag::NO_PROTECTION, 8)]
    #[case(DamageFlag::NO_TEAM_PROTECTION, 16)]
    fn damage_flag_is_the_flag_of_the_engine(#[case] flag: DamageFlag, #[case] value: i32) {
        assert_eq!(flag.__index__(), value);
    }

    #[test]
    fn flags_combine_and_mask_in_both_directions() {
        assert_eq!(CvarFlag::ARCHIVE.__or__(CvarFlag::LATCH as i32), 33);
        assert_eq!(CvarFlag::ARCHIVE.__ror__(32), 33);
        assert_eq!(CvarFlag::LATCH.__and__(33), 32);
        assert_eq!(CvarFlag::LATCH.__rand__(1), 0);
        assert_eq!(DamageFlag::RADIUS.__or__(DamageFlag::NO_ARMOR as i32), 3);
        assert_eq!(DamageFlag::NO_ARMOR.__and__(3), 2);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flags_work_with_operators_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

assert shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.LATCH == shinqlx.CVAR_ARCHIVE | shinqlx.CVAR_LATCH
assert isinstance(shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.LATCH, int)
assert 32 | shinqlx.CvarFlags.ARCHIVE == 33
assert 33 & shinqlx.CvarFlags.LATCH == 32
assert shinqlx.DamageFlags.RADIUS | shinqlx.DamageFlags.NO_ARMOR == 3
assert shinqlx.DamageFlags.NO_ARMOR == shinqlx.DAMAGE_NO_ARMOR
            "#,
                None,
                None,
            )
            .expect("this should not happen");
        });
    }
}
