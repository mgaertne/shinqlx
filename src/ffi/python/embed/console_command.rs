use crate::ffi::python::prelude::*;
use crate::quake_live_engine::ConsoleCommand;
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
pub(crate) fn pyshinqlx_console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.execute_console_command(cmd);

        Ok(())
    })
}

#[cfg(test)]
mod console_command_tests {
    use super::MAIN_ENGINE;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_console_command(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_command_with_main_engine_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("asdf"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| pyshinqlx_console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}
