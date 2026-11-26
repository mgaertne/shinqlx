#![allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    reason = "the members mirror the names of the C constants of the engine"
)]

use pyo3::{
    class::basic::CompareOp,
    types::{PyIterator, PyList},
};

use super::{game_enums::richcmp_with_index, prelude::*};
use crate::engine::prelude::*;

/// A Python class for a set of flags: `$set` is the `bitflags` type with the members, `$flags` the class around it.
///
/// The class works like Python's `enum.IntFlag`: members and sets combine with `|`, `&`, `^` and `~`, `in` tests a
/// flag, iterating gives the members that are set, and the value converts to and from integers. A value may hold bits
/// that are not members; they are kept.
macro_rules! py_flags {
    ($flags:ident, $set:ident, $name:literal, $doc:literal, [$($member:ident = $value:expr),+ $(,)?]) => {
        bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub(crate) struct $set: i32 {
                $(const $member = $value;)+
            }
        }

        impl core::fmt::Display for $set {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                bitflags::parser::to_writer(self, f)
            }
        }

        #[doc = $doc]
        #[pyclass(module = "_shinqlx", name = $name, frozen)]
        #[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, derive_more::Display)]
        #[display("{_0}")]
        pub(crate) struct $flags($set);

        #[pymethods]
        impl $flags {
            $(
                #[classattr]
                pub(crate) const $member: $flags = $flags($set::$member);
            )+

            #[new]
            #[pyo3(signature = (value = 0), text_signature = "(value=0)")]
            pub(crate) fn py_new(value: i32) -> Self {
                Self($set::from_bits_retain(value))
            }

            pub(crate) fn __index__(&self) -> i32 {
                self.0.bits()
            }

            fn __int__(&self) -> i32 {
                self.0.bits()
            }

            fn __hash__(&self) -> isize {
                self.0.bits() as isize
            }

            fn __bool__(&self) -> bool {
                !self.0.is_empty()
            }

            fn __richcmp__(
                slf: &Bound<'_, Self>,
                other: &Bound<'_, PyAny>,
                op: CompareOp,
            ) -> PyResult<Py<PyAny>> {
                let same = other.cast::<Self>().ok().map(|same| same.get().0.bits());
                richcmp_with_index(slf.py(), slf.get().0.bits(), same, other, op)
            }

            fn __or__(&self, other: i32) -> Self {
                Self(self.0 | $set::from_bits_retain(other))
            }

            fn __ror__(&self, other: i32) -> Self {
                self.__or__(other)
            }

            fn __and__(&self, other: i32) -> Self {
                Self(self.0 & $set::from_bits_retain(other))
            }

            fn __rand__(&self, other: i32) -> Self {
                self.__and__(other)
            }

            fn __xor__(&self, other: i32) -> Self {
                Self(self.0 ^ $set::from_bits_retain(other))
            }

            fn __rxor__(&self, other: i32) -> Self {
                self.__xor__(other)
            }

            fn __invert__(&self) -> Self {
                Self(!self.0)
            }

            fn __contains__(&self, other: i32) -> bool {
                self.0.contains($set::from_bits_retain(other))
            }

            fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
                PyList::new(py, self.0.iter().map(Self))?.try_iter()
            }

            fn __str__(&self) -> String {
                if self.0.is_empty() {
                    "0".to_string()
                } else {
                    self.to_string()
                }
            }

            fn __repr__(&self) -> String {
                let names: Vec<String> = self
                    .0
                    .iter_names()
                    .map(|(member, _)| format!("{}.{member}", $name))
                    .collect();
                let unknown = self.0.bits() & !$set::all().bits();
                match (names.is_empty(), unknown) {
                    (true, 0) => format!("{}(0)", $name),
                    (true, unknown) => format!("{}({unknown:#x})", $name),
                    (false, 0) => names.join("|"),
                    (false, unknown) => format!("{}|{unknown:#x}", names.join("|")),
                }
            }
        }
    };
}

py_flags!(
    CvarFlags,
    CvarFlagSet,
    "CvarFlags",
    "The flags of a cvar, e.g. for :func:`shinqlx.set_cvar`.\n\n\
     ``CvarFlags(33)`` is ``CvarFlags.ARCHIVE | CvarFlags.LATCH``. The old constants ``shinqlx.CVAR_*`` are plain \
     integers with the values of the members.",
    [
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
    ]
);

py_flags!(
    DamageFlags,
    DamageFlagSet,
    "DamageFlags",
    "The flags of the damage that a player takes, as passed to the ``damage`` event.\n\n\
     ``DamageFlags(3)`` is ``DamageFlags.RADIUS | DamageFlags.NO_ARMOR``. The old constants ``shinqlx.DAMAGE_*`` are \
     plain integers with the values of the members.",
    [
        RADIUS = DAMAGE_RADIUS as i32,
        NO_ARMOR = DAMAGE_NO_ARMOR as i32,
        NO_KNOCKBACK = DAMAGE_NO_KNOCKBACK as i32,
        NO_PROTECTION = DAMAGE_NO_PROTECTION as i32,
        NO_TEAM_PROTECTION = DAMAGE_NO_TEAM_PROTECTION as i32,
    ]
);

#[cfg(test)]
mod flag_enums_tests {
    use rstest::rstest;

    use super::{CvarFlags, DamageFlags};
    use crate::{prelude::*, python::prelude::*};

    #[rstest]
    #[case(CvarFlags::ARCHIVE, 1)]
    #[case(CvarFlags::USERINFO, 2)]
    #[case(CvarFlags::SERVERINFO, 4)]
    #[case(CvarFlags::SYSTEMINFO, 8)]
    #[case(CvarFlags::INIT, 16)]
    #[case(CvarFlags::LATCH, 32)]
    #[case(CvarFlags::ROM, 64)]
    #[case(CvarFlags::USER_CREATED, 128)]
    #[case(CvarFlags::TEMP, 256)]
    #[case(CvarFlags::CHEAT, 512)]
    #[case(CvarFlags::NORESTART, 1024)]
    fn cvar_flag_is_the_flag_of_the_engine(#[case] flag: CvarFlags, #[case] value: i32) {
        assert_eq!(flag.__index__(), value);
    }

    #[rstest]
    #[case(DamageFlags::RADIUS, 1)]
    #[case(DamageFlags::NO_ARMOR, 2)]
    #[case(DamageFlags::NO_KNOCKBACK, 4)]
    #[case(DamageFlags::NO_PROTECTION, 8)]
    #[case(DamageFlags::NO_TEAM_PROTECTION, 16)]
    fn damage_flag_is_the_flag_of_the_engine(#[case] flag: DamageFlags, #[case] value: i32) {
        assert_eq!(flag.__index__(), value);
    }

    #[test]
    fn flags_combine_to_flags() {
        let both = CvarFlags::ARCHIVE.__or__(CvarFlags::LATCH.__index__());

        assert_eq!(both, CvarFlags::py_new(33));
        assert_eq!(CvarFlags::ARCHIVE.__ror__(32), both);
        assert_eq!(both.__and__(32), CvarFlags::LATCH);
        assert_eq!(both.__rand__(1), CvarFlags::ARCHIVE);
        assert_eq!(both.__xor__(1), CvarFlags::LATCH);
        assert_eq!(DamageFlags::RADIUS.__or__(2), DamageFlags::py_new(3));
    }

    #[test]
    fn a_flag_is_contained_in_the_set_that_has_it() {
        let flags = CvarFlags::py_new(33);

        assert!(flags.__contains__(CvarFlags::LATCH.__index__()));
        assert!(flags.__contains__(33));
        assert!(!flags.__contains__(CvarFlags::ROM.__index__()));
        assert!(flags.__bool__());
        assert!(!CvarFlags::py_new(0).__bool__());
    }

    #[test]
    fn invert_stays_within_the_members() {
        let all = CvarFlags::py_new(2047);

        assert_eq!(CvarFlags::ARCHIVE.__invert__().__index__(), 2046);
        assert_eq!(all.__invert__(), CvarFlags::py_new(0));
    }

    #[test]
    fn repr_lists_the_members_and_keeps_unknown_bits() {
        assert_eq!(CvarFlags::ARCHIVE.__repr__(), "CvarFlags.ARCHIVE");
        assert_eq!(
            CvarFlags::py_new(33).__repr__(),
            "CvarFlags.ARCHIVE|CvarFlags.LATCH"
        );
        assert_eq!(CvarFlags::py_new(0).__repr__(), "CvarFlags(0)");
        assert_eq!(CvarFlags::py_new(4096).__repr__(), "CvarFlags(0x1000)");
        assert_eq!(
            CvarFlags::py_new(4097).__repr__(),
            "CvarFlags.ARCHIVE|0x1000"
        );
        assert_eq!(
            DamageFlags::py_new(3).__repr__(),
            "DamageFlags.RADIUS|DamageFlags.NO_ARMOR"
        );
    }

    #[test]
    fn display_names_the_flags_that_are_set() {
        assert_eq!(CvarFlags::py_new(33).to_string(), "ARCHIVE | LATCH");
        assert_eq!(CvarFlags::py_new(33).__str__(), "ARCHIVE | LATCH");
        assert_eq!(CvarFlags::py_new(4097).__str__(), "ARCHIVE | 0x1000");
        assert_eq!(CvarFlags::py_new(0).__str__(), "0");
        assert_eq!(DamageFlags::py_new(3).to_string(), "RADIUS | NO_ARMOR");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flags_work_with_operators_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

both = shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.LATCH
assert isinstance(both, shinqlx.CvarFlags)
assert both == shinqlx.CvarFlags(33) == 33 == shinqlx.CVAR_ARCHIVE | shinqlx.CVAR_LATCH
assert int(both) == 33 and hash(both) == hash(33)
assert 32 | shinqlx.CvarFlags.ARCHIVE == shinqlx.CvarFlags(33)
assert 33 & shinqlx.CvarFlags.LATCH == shinqlx.CvarFlags.LATCH
assert shinqlx.CvarFlags.LATCH in both and shinqlx.CvarFlags.ROM not in both
assert list(both) == [shinqlx.CvarFlags.ARCHIVE, shinqlx.CvarFlags.LATCH]
assert repr(both) == "CvarFlags.ARCHIVE|CvarFlags.LATCH"
assert str(both) == "ARCHIVE | LATCH" and f"{both}" == "ARCHIVE | LATCH"
assert ~both == shinqlx.CvarFlags(2047 - 33)
assert not shinqlx.CvarFlags(0) and not shinqlx.CvarFlags()
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
