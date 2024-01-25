use super::prelude::*;

#[pyclass(frozen)]
#[pyo3(module = "shinqlx", name = "Holdable")]
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

impl From<Holdable> for Option<String> {
    fn from(holdable: Holdable) -> Self {
        match holdable {
            Holdable::None => None,
            Holdable::Teleporter => Some("teleporter".into()),
            Holdable::MedKit => Some("medkit".into()),
            Holdable::Kamikaze => Some("kamikaze".into()),
            Holdable::Portal => Some("portal".into()),
            Holdable::Invulnerability => Some("invulnerability".into()),
            Holdable::Flight => Some("flight".into()),
            Holdable::Unknown => Some("unknown".into()),
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
    #[case(Holdable::None, None)]
    #[case(Holdable::Teleporter, Some("teleporter".into()))]
    #[case(Holdable::MedKit, Some("medkit".into()))]
    #[case(Holdable::Flight, Some("flight".into()))]
    #[case(Holdable::Kamikaze, Some("kamikaze".into()))]
    #[case(Holdable::Portal, Some("portal".into()))]
    #[case(Holdable::Invulnerability, Some("invulnerability".into()))]
    #[case(Holdable::Unknown, Some("unknown".into()))]
    fn opt_string_from_holdable(
        #[case] holdable: Holdable,
        #[case] expected_result: Option<String>,
    ) {
        assert_eq!(Option::<String>::from(holdable), expected_result);
    }
}
