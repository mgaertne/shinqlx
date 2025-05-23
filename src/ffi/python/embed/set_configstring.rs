use crate::ffi::python::{prelude::*, set_configstring};

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
pub(crate) fn pyshinqlx_set_configstring(
    py: Python<'_>,
    config_id: u16,
    value: &str,
) -> PyResult<()> {
    py.allow_threads(|| set_configstring(config_id, value))
}

#[cfg(test)]
mod set_configstring_tests {
    use mockall::predicate;
    use pyo3::exceptions::PyValueError;
    use rstest::rstest;

    use crate::{
        ffi::python::prelude::*, hooks::mock_hooks::shinqlx_set_configstring_context, prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_index_out_of_bounds(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_configstring(py, 2048, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_proper_index(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(666), predicate::eq("asdf"))
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_set_configstring(py, 666, "asdf"));
        assert!(result.is_ok());
    }
}
