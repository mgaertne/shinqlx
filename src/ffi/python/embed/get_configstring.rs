use crate::ffi::python::prelude::*;

use crate::ffi::python::get_configstring;

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
pub(crate) fn pyshinqlx_get_configstring(py: Python<'_>, config_id: u16) -> PyResult<String> {
    py.allow_threads(|| get_configstring(config_id))
}

#[cfg(test)]
mod get_configstring_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_configstring_for_too_large_configstring_id(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_configstring(py, MAX_CONFIGSTRINGS as u16 + 1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_configstring_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_configstring(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_configstring_forwards_call_to_engine(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(666, "asdf".to_string(), 1)
            .run(|| {
                let result = Python::with_gil(|py| pyshinqlx_get_configstring(py, 666));
                assert_eq!(result.expect("result was not OK"), "asdf");
            });
    }
}
