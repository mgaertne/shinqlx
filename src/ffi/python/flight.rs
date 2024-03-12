use super::prelude::*;

use pyo3::{basic::CompareOp, exceptions::PyValueError, types::PyTuple};

/// A struct sequence containing parameters for the flight holdable item.
#[pyclass(frozen)]
#[pyo3(module = "shinqlx", name = "Flight", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) struct Flight(
    #[pyo3(name = "fuel")] pub(crate) i32,
    #[pyo3(name = "max_fuel")] pub(crate) i32,
    #[pyo3(name = "thrust")] pub(crate) i32,
    #[pyo3(name = "refuel")] pub(crate) i32,
);

impl From<Flight> for [i32; 4] {
    fn from(flight: Flight) -> Self {
        [flight.0, flight.1, flight.2, flight.3]
    }
}

#[pymethods]
impl Flight {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 4 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 4 flight parameters",
            ));
        }

        if values.len() > 4 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 4 flight parameters",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Flight values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
            results[3].unwrap(),
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
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }
}

#[cfg(test)]
mod flight_tests {
    use crate::ffi::python::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run_bound(
                r#"
import _shinqlx
flight = _shinqlx.Flight((0, 1, 2, 3))
            "#,
                None,
                None,
            );
            assert!(
                flight_constructor.is_ok(),
                "{}",
                flight_constructor.expect_err("this should not happen")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_py_constructor_with_too_few_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run_bound(
                r#"
import _shinqlx
flight = _shinqlx.Flight((0, 1, 2))
            "#,
                None,
                None,
            );
            assert!(flight_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_py_constructor_with_too_many_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run_bound(
                r#"
import _shinqlx
flight = _shinqlx.Flight((0, 1, 2, 3, 4))
            "#,
                None,
                None,
            );
            assert!(flight_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_py_constructor_with_non_numeric_values(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run_bound(
                r#"
import _shinqlx
flight = _shinqlx.Flight(("asdf", True, (1, 2, 3), []))
            "#,
                None,
                None,
            );
            assert!(flight_constructor.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_can_be_compared_for_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Flight((0, 1, 2, 3)) == _shinqlx.Flight((0, 1, 2, 3)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_can_be_compared_for_non_equality_in_python(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Flight((0, 1, 2, 3)) != _shinqlx.Flight((3, 2, 1, 0)))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_can_not_be_compared_for_lower_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run_bound(
                r#"
import _shinqlx
assert(_shinqlx.Flight((0, 1, 2, 3)) < _shinqlx.Flight((3, 2, 1, 0)))
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }

    #[test]
    fn flight_to_str() {
        let flight = Flight(1, 2, 3, 4);
        assert_eq!(
            flight.__str__(),
            "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)"
        );
    }

    #[test]
    fn flight_repr() {
        let flight = Flight(1, 2, 3, 4);
        assert_eq!(
            flight.__repr__(),
            "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)"
        );
    }
}
