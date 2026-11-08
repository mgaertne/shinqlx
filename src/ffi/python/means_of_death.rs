#![allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    reason = "the variants mirror the names of the C constants of the engine"
)]

use pyo3::class::basic::CompareOp;

use super::{game_enums::richcmp_with_index, prelude::*};
use crate::ffi::c::prelude::*;

/// The causes of a death, e.g. the ``means_of_death`` of the ``damage`` event and the argument of
/// :meth:`shinqlx.Player.slay_with_mod`.
///
/// The old constants ``shinqlx.MOD_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "MeansOfDeath", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum MeansOfDeath {
    UNKNOWN = meansOfDeath_t::MOD_UNKNOWN as i32,
    SHOTGUN = meansOfDeath_t::MOD_SHOTGUN as i32,
    GAUNTLET = meansOfDeath_t::MOD_GAUNTLET as i32,
    MACHINEGUN = meansOfDeath_t::MOD_MACHINEGUN as i32,
    GRENADE = meansOfDeath_t::MOD_GRENADE as i32,
    GRENADE_SPLASH = meansOfDeath_t::MOD_GRENADE_SPLASH as i32,
    ROCKET = meansOfDeath_t::MOD_ROCKET as i32,
    ROCKET_SPLASH = meansOfDeath_t::MOD_ROCKET_SPLASH as i32,
    PLASMA = meansOfDeath_t::MOD_PLASMA as i32,
    PLASMA_SPLASH = meansOfDeath_t::MOD_PLASMA_SPLASH as i32,
    RAILGUN = meansOfDeath_t::MOD_RAILGUN as i32,
    LIGHTNING = meansOfDeath_t::MOD_LIGHTNING as i32,
    BFG = meansOfDeath_t::MOD_BFG as i32,
    BFG_SPLASH = meansOfDeath_t::MOD_BFG_SPLASH as i32,
    WATER = meansOfDeath_t::MOD_WATER as i32,
    SLIME = meansOfDeath_t::MOD_SLIME as i32,
    LAVA = meansOfDeath_t::MOD_LAVA as i32,
    CRUSH = meansOfDeath_t::MOD_CRUSH as i32,
    TELEFRAG = meansOfDeath_t::MOD_TELEFRAG as i32,
    FALLING = meansOfDeath_t::MOD_FALLING as i32,
    SUICIDE = meansOfDeath_t::MOD_SUICIDE as i32,
    TARGET_LASER = meansOfDeath_t::MOD_TARGET_LASER as i32,
    TRIGGER_HURT = meansOfDeath_t::MOD_TRIGGER_HURT as i32,
    NAIL = meansOfDeath_t::MOD_NAIL as i32,
    CHAINGUN = meansOfDeath_t::MOD_CHAINGUN as i32,
    PROXIMITY_MINE = meansOfDeath_t::MOD_PROXIMITY_MINE as i32,
    KAMIKAZE = meansOfDeath_t::MOD_KAMIKAZE as i32,
    JUICED = meansOfDeath_t::MOD_JUICED as i32,
    GRAPPLE = meansOfDeath_t::MOD_GRAPPLE as i32,
    SWITCH_TEAMS = meansOfDeath_t::MOD_SWITCH_TEAMS as i32,
    THAW = meansOfDeath_t::MOD_THAW as i32,
    LIGHTNING_DISCHARGE = meansOfDeath_t::MOD_LIGHTNING_DISCHARGE as i32,
    HMG = meansOfDeath_t::MOD_HMG as i32,
    RAILGUN_HEADSHOT = meansOfDeath_t::MOD_RAILGUN_HEADSHOT as i32,
}

#[pymethods]
impl MeansOfDeath {
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
}

#[cfg(test)]
mod means_of_death_tests {
    use rstest::rstest;

    use super::MeansOfDeath;
    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[case(MeansOfDeath::UNKNOWN, meansOfDeath_t::MOD_UNKNOWN)]
    #[case(MeansOfDeath::SHOTGUN, meansOfDeath_t::MOD_SHOTGUN)]
    #[case(MeansOfDeath::ROCKET_SPLASH, meansOfDeath_t::MOD_ROCKET_SPLASH)]
    #[case(MeansOfDeath::RAILGUN, meansOfDeath_t::MOD_RAILGUN)]
    #[case(MeansOfDeath::TELEFRAG, meansOfDeath_t::MOD_TELEFRAG)]
    #[case(MeansOfDeath::SWITCH_TEAMS, meansOfDeath_t::MOD_SWITCH_TEAMS)]
    #[case(MeansOfDeath::HMG, meansOfDeath_t::MOD_HMG)]
    #[case(MeansOfDeath::RAILGUN_HEADSHOT, meansOfDeath_t::MOD_RAILGUN_HEADSHOT)]
    fn means_of_death_is_the_value_of_the_engine(
        #[case] means_of_death: MeansOfDeath,
        #[case] engine_value: meansOfDeath_t,
    ) {
        assert_eq!(means_of_death.__index__(), engine_value as i32);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn means_of_death_is_registered_and_usable_as_integer(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

assert shinqlx.MeansOfDeath.ROCKET == shinqlx.MOD_ROCKET
assert shinqlx.MeansOfDeath.RAILGUN_HEADSHOT == 33
assert shinqlx.MeansOfDeath.ROCKET != shinqlx.MeansOfDeath.PLASMA
assert [i for i in range(40)][shinqlx.MeansOfDeath.BFG] == 12
            "#,
                None,
                None,
            )
            .expect("this should not happen");
        });
    }
}
