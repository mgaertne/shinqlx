use super::prelude::*;

use core::array;
use pyo3::{basic::CompareOp, exceptions::PyValueError, types::PyTuple};

#[pyclass]
struct Vector3Iter {
    iter: array::IntoIter<i32, 3>,
}

#[pymethods]
impl Vector3Iter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<i32> {
        slf.iter.next()
    }
}

/// A three-dimensional vector.
#[pyclass(name = "Vector3", module = "shinqlx", get_all, frozen)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, Default)]
pub(crate) struct Vector3(
    #[pyo3(name = "x")] pub(crate) i32,
    #[pyo3(name = "y")] pub(crate) i32,
    #[pyo3(name = "z")] pub(crate) i32,
);

#[pymethods]
impl Vector3 {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 3 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all three dimensions",
            ));
        }

        if values.len() > 3 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than three dimensions",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Vector3 values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    pub(crate) fn __str__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __repr__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<Vector3Iter>> {
        let iter_array = [slf.0, slf.1, slf.2];
        let iter = Vector3Iter {
            iter: iter_array.into_iter(),
        };
        Py::new(slf.py(), iter)
    }
}

impl From<(f32, f32, f32)> for Vector3 {
    fn from(value: (f32, f32, f32)) -> Self {
        Self(value.0 as i32, value.1 as i32, value.2 as i32)
    }
}

#[cfg(test)]
mod vector3_tests {
    use crate::ffi::python::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_tuple_test(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("_shinqlx").expect("this should not happen");
            let vector3 = shinqlx_module
                .getattr("Vector3")
                .expect("this should not happen");
            let tuple = py
                .import_bound("builtins")
                .expect("this should not happen")
                .getattr("tuple")
                .expect("this should not happen");
            assert!(vector3
                .is_instance(&tuple.get_type())
                .expect("result was not OK"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run_bound(
                r#"
import _shinqlx
weapons = _shinqlx.Vector3((0, 42, 666))
            "#,
                None,
                None,
            );
            assert!(
                vector3_constructor.is_ok(),
                "{}",
                vector3_constructor.expect_err("this should not happen")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_py_constructor_with_too_few_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run_bound(
                r#"
import _shinqlx
powerups = _shinqlx.Vector3((0, 1))
            "#,
                None,
                None,
            );
            assert!(vector3_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_py_constructor_with_too_many_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run_bound(
                r#"
import _shinqlx
powerups = _shinqlx.Vector3((0, 1, 2, 3))
            "#,
                None,
                None,
            );
            assert!(vector3_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_py_constructor_with_non_numeric_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run_bound(
                r#"
import _shinqlx
powerups = _shinqlx.Vector3(("asdf", True, (1, 2, 3)))
            "#,
                None,
                None,
            );
            assert!(vector3_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_can_be_compared_for_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Vector3((0, 1, 2)) == _shinqlx.Vector3((0, 1, 2)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_can_be_compared_for_non_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Vector3((0, 1, 2)) != _shinqlx.Vector3((2, 1, 0)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_can_not_be_compared_for_lower_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Vector3((0, 1, 2)) < _shinqlx.Vector3((2, 1, 0)))
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn vector3_can_be_iterated_over_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
vector3 = _shinqlx.Vector3((0, 1, 2))
vec_iter = iter(iter(vector3))
assert(next(vec_iter) == 0)
assert(next(vec_iter) == 1)
assert(next(vec_iter) == 2)
try:
    next(vec_iter)
except StopIteration:
    pass
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    fn vector3_to_str() {
        let vector3 = Vector3(1, 2, 3);
        assert_eq!(vector3.__str__(), "Vector3(x=1, y=2, z=3)");
    }

    #[test]
    fn vector3_repr() {
        let vector3 = Vector3(1, 2, 3);
        assert_eq!(vector3.__repr__(), "Vector3(x=1, y=2, z=3)");
    }

    #[test]
    fn vector3_from_tuple() {
        assert_eq!(Vector3::from((1.0, 2.0, 3.0)), Vector3(1, 2, 3));
    }
}
