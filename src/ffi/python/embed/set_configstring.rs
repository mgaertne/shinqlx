#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;

use crate::prelude::MAX_CONFIGSTRINGS;
use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, PyResult, Python};

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
pub(crate) fn minqlx_set_configstring(py: Python<'_>, config_id: u32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(config_id, value.to_string());

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_configstring_tests {
    use super::minqlx_set_configstring;
    use crate::hooks::mock_hooks::shinqlx_set_configstring_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_configstring_with_index_out_of_bounds() {
        Python::with_gil(|py| {
            let result = minqlx_set_configstring(py, 2048, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_configstring_with_proper_index() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(666), predicate::eq("asdf".to_string()))
            .times(1);

        let result = Python::with_gil(|py| minqlx_set_configstring(py, 666, "asdf"));
        assert!(result.is_ok());
    }
}