use core::hint::cold_path;

use arrayvec::ArrayVec;
use derive_more::Display;
use pyo3::{exceptions::PyValueError, types::PyTuple};
use rayon::prelude::*;

use super::prelude::*;

/// A struct sequence containing all the weapons in the game.
#[pyclass(
    module = "_shinqlx",
    name = "Weapons",
    frozen,
    get_all,
    sequence,
    eq,
    str
)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, Display)]
#[display(
    "Weapons(g={_0}, mg={_1}, sg={_2}, gl={_3}, rl={_4}, lg={_5}, rg={_6}, pg={_7}, bfg={_8}, gh={_9}, ng={_10}, pl={_11}, cg={_12}, hmg={_13}, hands={_14})"
)]
pub(crate) struct Weapons(
    #[pyo3(name = "g")] pub(crate) i32,
    #[pyo3(name = "mg")] pub(crate) i32,
    #[pyo3(name = "sg")] pub(crate) i32,
    #[pyo3(name = "gl")] pub(crate) i32,
    #[pyo3(name = "rl")] pub(crate) i32,
    #[pyo3(name = "lg")] pub(crate) i32,
    #[pyo3(name = "rg")] pub(crate) i32,
    #[pyo3(name = "pg")] pub(crate) i32,
    #[pyo3(name = "bfg")] pub(crate) i32,
    #[pyo3(name = "gh")] pub(crate) i32,
    #[pyo3(name = "ng")] pub(crate) i32,
    #[pyo3(name = "pl")] pub(crate) i32,
    #[pyo3(name = "cg")] pub(crate) i32,
    #[pyo3(name = "hmg")] pub(crate) i32,
    #[pyo3(name = "hands")] pub(crate) i32,
);

impl From<[i32; 15]> for Weapons {
    fn from(value: [i32; 15]) -> Self {
        Self(
            value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
            value[8], value[9], value[10], value[11], value[12], value[13], value[14],
        )
    }
}

impl From<Weapons> for [i32; 15] {
    fn from(value: Weapons) -> Self {
        [
            value.0, value.1, value.2, value.3, value.4, value.5, value.6, value.7, value.8,
            value.9, value.10, value.11, value.12, value.13, value.14,
        ]
    }
}

#[pymethods]
impl Weapons {
    #[new]
    fn py_new(values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if values.len() < 15 {
            cold_path();
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 15 weapons",
            ));
        }

        if values.len() > 15 {
            cold_path();
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 15 weapons",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<ArrayVec<Option<i32>, 15>>();

        if results.par_iter().any(|item| item.is_none()) {
            cold_path();
            return Err(PyValueError::new_err("Weapons values need to be boolean"));
        }

        Ok(Self::from(
            results
                .iter()
                .map(|&value| value.unwrap_or(0))
                .collect::<ArrayVec<i32, 15>>()
                .into_inner()
                .unwrap(),
        ))
    }

    fn __repr__(&self) -> String {
        format!("{self}")
    }
}

#[cfg(test)]
mod weapons_tests {
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

    use crate::ffi::python::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor =py.run(cr#"
import shinqlx
weapons = shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False))
            "#, None, None);
            assert!(
                weapons_constructor.is_ok(),
                "{}",
                weapons_constructor.expect_err("this should not happen")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_py_constructor_with_too_few_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False))
            "#,
                None,
                None,
            );
            assert!(weapons_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_py_constructor_with_too_many_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False, True))
            "#,
                None,
                None,
            );
            assert!(weapons_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_py_constructor_with_non_boolean_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons(("asdf", True, (1, 2, 3), [], {}, set(), 6, 7, 8, 9, 10, 11, 12, 13, 14))
            "#,
                None,
                None,
            );
            assert!(weapons_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_can_be_compared_for_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False)) == shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_can_be_compared_for_non_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False)) != shinqlx.Weapons((True, True, True, True, True, True, True, True, True, True, True, True, True, True, True)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn weapons_can_not_be_compared_for_lower_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False)) < shinqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False)))
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}

#[cfg(test)]
mod ammo_tests {
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;
    use tap::Conv;

    use crate::ffi::python::prelude::*;

    #[test]
    fn weapons_from_integer_array() {
        assert_eq!(
            Weapons::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]),
            Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)
        );
    }

    #[test]
    fn weapons_into_integer_array() {
        assert_eq!(
            Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15).conv::<[i32; 15]>(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                cr#"
import shinqlx
weapons = shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14))
            "#,
                None,
                None,
            );
            assert!(
                ammo_constructor.is_ok(),
                "{}",
                ammo_constructor.expect_err("this should not happen")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_py_constructor_with_too_few_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13))
            "#,
                None,
                None,
            );
            assert!(ammo_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_py_constructor_with_too_many_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15))
            "#,
                None,
                None,
            );
            assert!(ammo_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_py_constructor_with_non_numeric_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                cr#"
import shinqlx
powerups = shinqlx.Weapons(("asdf", True, (1, 2, 3), [], {}, set(), 6, 7, 8, 9, 10, 11, 12, 13, 14))
            "#,
                None,
                None,
            );
            assert!(ammo_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_can_be_compared_for_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)) == shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_can_be_compared_for_non_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)) != shinqlx.Weapons((14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn ammo_can_not_be_compared_for_lower_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
assert(shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)) < shinqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)))
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }

    #[test]
    fn ammo_to_str() {
        let ammo = Weapons(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
        assert_eq!(
            format!("{ammo}"),
            "Weapons(g=0, mg=1, sg=2, gl=3, rl=4, lg=5, rg=6, pg=7, bfg=8, gh=9, ng=10, pl=11, cg=12, hmg=13, hands=14)"
        );
    }

    #[test]
    fn ammo_repr() {
        let ammo = Weapons(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
        assert_eq!(
            ammo.__repr__(),
            "Weapons(g=0, mg=1, sg=2, gl=3, rl=4, lg=5, rg=6, pg=7, bfg=8, gh=9, ng=10, pl=11, cg=12, hmg=13, hands=14)"
        );
    }
}
