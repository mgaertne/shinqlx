use super::prelude::*;

use core::fmt::{Display, Formatter};

use arrayvec::ArrayVec;

use pyo3::{exceptions::PyValueError, types::PyTuple};

/// A struct sequence containing all the powerups in the game.
#[pyclass(
    module = "_shinqlx",
    name = "Powerups",
    frozen,
    get_all,
    sequence,
    eq,
    str
)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) struct Powerups(
    #[pyo3(name = "quad")] pub(crate) i32,
    #[pyo3(name = "battlesuit")] pub(crate) i32,
    #[pyo3(name = "haste")] pub(crate) i32,
    #[pyo3(name = "invisibility")] pub(crate) i32,
    #[pyo3(name = "regeneration")] pub(crate) i32,
    #[pyo3(name = "invulnerability")] pub(crate) i32,
);

impl From<[i32; 6]> for Powerups {
    fn from(value: [i32; 6]) -> Self {
        Self(value[0], value[1], value[2], value[3], value[4], value[5])
    }
}

impl From<Powerups> for [i32; 6] {
    fn from(value: Powerups) -> Self {
        [value.0, value.1, value.2, value.3, value.4, value.5]
    }
}

impl Display for Powerups {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5
        )
    }
}

#[pymethods]
impl Powerups {
    #[new]
    fn py_new(values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if values.len() < 6 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 6 powerups",
            ));
        }

        if values.len() > 6 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 6 powerups",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<ArrayVec<Option<i32>, 6>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Powerups values need to be integer"));
        }

        Ok(Self::from(
            results
                .iter()
                .map(|&value| value.unwrap_or(0))
                .collect::<ArrayVec<i32, 6>>()
                .into_inner()
                .unwrap(),
        ))
    }

    fn __repr__(&self) -> String {
        format!(
            "Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5
        )
    }
}

#[cfg(test)]
mod powerups_tests {
    use crate::ffi::python::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

    #[test]
    fn powerups_from_integer_array() {
        assert_eq!(
            Powerups::from([1, 2, 3, 4, 5, 6]),
            Powerups(1, 2, 3, 4, 5, 6)
        );
    }

    #[test]
    fn powerups_into_integer_array() {
        assert_eq!(
            <Powerups as Into<[i32; 6]>>::into(Powerups(1, 2, 3, 4, 5, 6)),
            [1, 2, 3, 4, 5, 6]
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                cr#"
import _shinqlx
powerups = _shinqlx.Powerups((0, 1, 2, 3, 4, 5))
            "#,
                None,
                None,
            );
            assert!(
                powerups_constructor.is_ok(),
                "{}",
                powerups_constructor.expect_err("this should not happen"),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_py_constructor_with_too_few_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                cr#"
import _shinqlx
powerups = _shinqlx.Powerups((0, 1, 2, 3, 4))
            "#,
                None,
                None,
            );
            assert!(powerups_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_py_constructor_with_too_many_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                cr#"
import _shinqlx
powerups = _shinqlx.Powerups((0, 1, 2, 3, 4, 5, 6))
            "#,
                None,
                None,
            );
            assert!(powerups_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_py_constructor_with_non_numeric_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                cr#"
import _shinqlx
powerups = _shinqlx.Powerups(("asdf", True, (1, 2, 3), [], {}, set()))
            "#,
                None,
                None,
            );
            assert!(powerups_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_can_be_compared_for_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import _shinqlx
assert(_shinqlx.Powerups((0, 1, 2, 3, 4, 5)) == _shinqlx.Powerups((0, 1, 2, 3, 4, 5)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_can_be_compared_for_non_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import _shinqlx
assert(_shinqlx.Powerups((0, 1, 2, 3, 4, 5)) != _shinqlx.Powerups((5, 4, 3, 2, 1, 0)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn powerups_can_not_be_compared_for_lower_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import _shinqlx
assert(_shinqlx.Powerups((0, 1, 2, 3, 4, 5)) < _shinqlx.Powerups((5, 4, 3, 2, 1, 0)))
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }

    #[test]
    fn powerups_to_str() {
        let powerups = Powerups(1, 2, 3, 4, 5, 6);
        assert_eq!(
            format!("{powerups}"),
            "Powerups(quad=1, battlesuit=2, haste=3, invisibility=4, regeneration=5, invulnerability=6)"
        );
    }

    #[test]
    fn powerups_repr() {
        let powerups = Powerups(1, 2, 3, 4, 5, 6);
        assert_eq!(
            powerups.__repr__(),
            "Powerups(quad=1, battlesuit=2, haste=3, invisibility=4, regeneration=5, invulnerability=6)"
        );
    }
}
