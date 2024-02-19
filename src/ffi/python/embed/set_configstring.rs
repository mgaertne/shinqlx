use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;

use pyo3::exceptions::PyValueError;

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
pub(crate) fn pyshinqlx_set_configstring(
    py: Python<'_>,
    config_id: u32,
    value: &str,
) -> PyResult<()> {
    py.allow_threads(|| {
        if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
            return Err(PyValueError::new_err(format!(
                "index needs to be a number from 0 to {}.",
                MAX_CONFIGSTRINGS - 1
            )));
        }

        shinqlx_set_configstring(config_id, value);

        Ok(())
    })
}

#[cfg(test)]
mod set_configstring_tests {
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_set_configstring_context;
    use crate::prelude::*;

    use mockall::predicate;
    use pyo3::exceptions::PyValueError;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_index_out_of_bounds() {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_configstring(py, 2048, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_proper_index() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(666), predicate::eq("asdf"))
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_set_configstring(py, 666, "asdf"));
        assert!(result.is_ok());
    }
}
