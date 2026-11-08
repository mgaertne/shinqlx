use derive_more::Display;
use pyo3::class::basic::CompareOp;

use super::{
    game_enums::{compare_with_name_or_index, richcmp_result},
    prelude::*,
};

/// The holdable items of Quake Live.
///
/// The string of a member is the name that :attr:`shinqlx.Player.holdable` accepts. The integer
/// value is the item index of the engine.
#[pyclass(module = "_shinqlx", name = "Holdable", frozen, str)]
#[derive(PartialEq, Debug, Clone, Copy, Display)]
#[repr(i32)]
pub(crate) enum Holdable {
    #[display("None")]
    #[pyo3(name = "NONE")]
    None = 0,
    #[display("teleporter")]
    #[pyo3(name = "TELEPORTER")]
    Teleporter = 27,
    #[display("medkit")]
    #[pyo3(name = "MEDKIT")]
    MedKit = 28,
    #[display("kamikaze")]
    #[pyo3(name = "KAMIKAZE")]
    Kamikaze = 37,
    #[display("portal")]
    #[pyo3(name = "PORTAL")]
    Portal = 38,
    #[display("invulnerability")]
    #[pyo3(name = "INVULNERABILITY")]
    Invulnerability = 39,
    #[display("flight")]
    #[pyo3(name = "FLIGHT")]
    Flight = 34,
    #[display("unknown")]
    #[pyo3(name = "UNKNOWN")]
    Unknown = 666,
}

#[pymethods]
impl Holdable {
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
        let this = slf.get();
        let equal = other.cast::<Self>().map_or_else(
            |_| compare_with_name_or_index(*this as i32, &this.to_string().to_lowercase(), other),
            |same| Some(same.get() == this),
        );
        richcmp_result(slf.py(), equal, op)
    }
}

impl From<i32> for Holdable {
    fn from(value: i32) -> Self {
        match value {
            0 => Holdable::None,
            27 => Holdable::Teleporter,
            28 => Holdable::MedKit,
            34 => Holdable::Flight,
            37 => Holdable::Kamikaze,
            38 => Holdable::Portal,
            39 => Holdable::Invulnerability,
            _ => Holdable::Unknown,
        }
    }
}

impl From<Holdable> for i32 {
    fn from(value: Holdable) -> Self {
        match value {
            Holdable::None => 0,
            Holdable::Teleporter => 27,
            Holdable::MedKit => 28,
            Holdable::Flight => 34,
            Holdable::Kamikaze => 37,
            Holdable::Portal => 38,
            Holdable::Invulnerability => 39,
            Holdable::Unknown => 0,
        }
    }
}

impl From<Option<&str>> for Holdable {
    fn from(holdable_str: Option<&str>) -> Self {
        match holdable_str {
            None | Some("none") => Holdable::None,
            Some("teleporter") => Holdable::Teleporter,
            Some("medkit") => Holdable::MedKit,
            Some("kamikaze") => Holdable::Kamikaze,
            Some("portal") => Holdable::Portal,
            Some("invulnerability") => Holdable::Invulnerability,
            Some("flight") => Holdable::Flight,
            _ => Holdable::Unknown,
        }
    }
}

#[cfg(test)]
mod holdable_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::Holdable;

    #[rstest]
    #[case(0, Holdable::None)]
    #[case(27, Holdable::Teleporter)]
    #[case(28, Holdable::MedKit)]
    #[case(34, Holdable::Flight)]
    #[case(37, Holdable::Kamikaze)]
    #[case(38, Holdable::Portal)]
    #[case(39, Holdable::Invulnerability)]
    #[case(666, Holdable::Unknown)]
    fn holdable_from_integer(#[case] integer: i32, #[case] expected_holdable: Holdable) {
        assert_eq!(Holdable::from(integer), expected_holdable);
    }

    #[rstest]
    #[case(Holdable::None, 0)]
    #[case(Holdable::Teleporter, 27)]
    #[case(Holdable::MedKit, 28)]
    #[case(Holdable::Flight, 34)]
    #[case(Holdable::Kamikaze, 37)]
    #[case(Holdable::Portal, 38)]
    #[case(Holdable::Invulnerability, 39)]
    #[case(Holdable::Unknown, 0)]
    fn integer_from_holdable(#[case] holdable: Holdable, #[case] expected_integer: i32) {
        assert_eq!(i32::from(holdable), expected_integer);
    }

    #[rstest]
    #[case(Holdable::None, "None")]
    #[case(Holdable::Teleporter, "teleporter")]
    #[case(Holdable::MedKit, "medkit")]
    #[case(Holdable::Flight, "flight")]
    #[case(Holdable::Kamikaze, "kamikaze")]
    #[case(Holdable::Portal, "portal")]
    #[case(Holdable::Invulnerability, "invulnerability")]
    #[case(Holdable::Unknown, "unknown")]
    fn opt_string_from_holdable(#[case] holdable: Holdable, #[case] expected_result: &str) {
        assert_eq!(format!("{holdable}"), expected_result);
    }

    #[rstest]
    #[case(None, Holdable::None)]
    #[case(Some("none"), Holdable::None)]
    #[case(Some("teleporter"), Holdable::Teleporter)]
    #[case(Some("medkit"), Holdable::MedKit)]
    #[case(Some("flight"), Holdable::Flight)]
    #[case(Some("kamikaze"), Holdable::Kamikaze)]
    #[case(Some("portal"), Holdable::Portal)]
    #[case(Some("invulnerability"), Holdable::Invulnerability)]
    #[case(Some("unknown"), Holdable::Unknown)]
    #[case(Some("asdf"), Holdable::Unknown)]
    fn holdable_from_opt_string(
        #[case] holdable_str: Option<&str>,
        #[case] expected_holdable: Holdable,
    ) {
        assert_eq!(Holdable::from(holdable_str), expected_holdable);
    }
}
