use core::hint::cold_path;

use arrayvec::ArrayVec;
use derive_more::Display;
use pyo3::{exceptions::PyValueError, types::PyTuple};

use super::prelude::*;

/// A struct sequence containing parameters for the flight holdable item.
#[pyclass(
    module = "_shinqlx",
    name = "Flight",
    frozen,
    get_all,
    sequence,
    eq,
    str
)]
#[derive(PartialEq, Debug, Clone, Copy, Display)]
#[display("Flight(fuel={_0}, max_fuel={_1}, thrust={_2}, refuel={_3})")]
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
    fn py_new(values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if values.len() < 4 {
            cold_path();
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 4 flight parameters",
            ));
        }

        if values.len() > 4 {
            cold_path();
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 4 flight parameters",
            ));
        }

        let results = values
            .iter()
            .filter_map(|item| item.extract::<i32>().ok())
            .collect::<ArrayVec<i32, 4>>();

        if results.len() != 4 {
            cold_path();
            return Err(PyValueError::new_err("Flight values need to be integer"));
        }

        Ok(Self(results[0], results[1], results[2], results[3]))
    }

    fn __repr__(&self) -> String {
        format!("{self}")
    }
}

#[cfg(test)]
mod flight_tests {
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

    use crate::ffi::python::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run(
                cr#"
import shinqlx
flight = shinqlx.Flight((0, 1, 2, 3))
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
            let flight_constructor = py.run(
                cr#"
import shinqlx
flight = shinqlx.Flight((0, 1, 2))
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
            let flight_constructor = py.run(
                cr#"
import shinqlx
flight = shinqlx.Flight((0, 1, 2, 3, 4))
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
            let flight_constructor = py.run(
                cr#"
import shinqlx
flight = shinqlx.Flight(("asdf", True, (1, 2, 3), []))
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
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Flight((0, 1, 2, 3)) == shinqlx.Flight((0, 1, 2, 3)))
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
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Flight((0, 1, 2, 3)) != shinqlx.Flight((3, 2, 1, 0)))
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
            let result = py.run(
                cr#"
import shinqlx
assert(shinqlx.Flight((0, 1, 2, 3)) < shinqlx.Flight((3, 2, 1, 0)))
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
            format!("{flight}"),
            "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)"
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_to_str_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
assert(str(shinqlx.Flight((1, 2, 3, 4))) == "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)")
            "#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }

    #[test]
    fn flight_repr() {
        let flight = Flight(1, 2, 3, 4);
        assert_eq!(
            flight.__repr__(),
            "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)"
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn flight_repr_in_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
assert(repr(shinqlx.Flight((1, 2, 3, 4))) == "Flight(fuel=1, max_fuel=2, thrust=3, refuel=4)")
            "#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}
