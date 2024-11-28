use super::prelude::*;

use core::fmt::{Display, Formatter};

#[pyclass(module = "_shinqlx", name = "Holdable", frozen, eq, eq_int, str)]
#[derive(PartialEq, Debug, Clone, Copy)]
pub(crate) enum Holdable {
    None = 0,
    Teleporter = 27,
    MedKit = 28,
    Kamikaze = 37,
    Portal = 38,
    Invulnerability = 39,
    Flight = 34,
    Unknown = 666,
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

impl Display for Holdable {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Holdable::None => write!(f, "None"),
            Holdable::Teleporter => write!(f, "teleporter"),
            Holdable::MedKit => write!(f, "medkit"),
            Holdable::Kamikaze => write!(f, "kamikaze"),
            Holdable::Portal => write!(f, "portal"),
            Holdable::Invulnerability => write!(f, "invulnerability"),
            Holdable::Flight => write!(f, "flight"),
            Holdable::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod holdable_tests {
    use super::Holdable;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

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
}
