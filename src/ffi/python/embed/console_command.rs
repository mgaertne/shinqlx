use crate::ffi::python::console_command;
use crate::ffi::python::prelude::*;

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
pub(crate) fn pyshinqlx_console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
    py.allow_threads(|| console_command(cmd))
}

#[cfg(test)]
mod console_command_tests {
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_console_command(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_command_with_main_engine_set(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("asdf", 1)
            .run(|| {
                let result = Python::with_gil(|py| pyshinqlx_console_command(py, "asdf"));
                assert!(result.is_ok());
            });
    }
}
