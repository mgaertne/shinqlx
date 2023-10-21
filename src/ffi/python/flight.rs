use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// A struct sequence containing parameters for the flight holdable item.
#[pyclass]
#[pyo3(module = "minqlx", name = "Flight", get_all)]
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
#[cfg(not(miri))]
mod flight_tests {
    use crate::ffi::python::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn flight_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Flight((0, 1, 2, 3))
            "#,
                None,
                None,
            );
            assert!(
                flight_constructor.is_ok(),
                "{}",
                flight_constructor.err().unwrap()
            );
        });
    }
}
