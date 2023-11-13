use crate::quake_live_engine::AddCommand;
use crate::MAIN_ENGINE;

use crate::commands::cmd_py_command;
use pyo3::exceptions::PyEnvironmentError;
use pyo3::{pyfunction, PyResult, Python};

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
pub(crate) fn pyshinqlx_add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.add_command(command, cmd_py_command);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod add_console_command_tests {
    use super::pyshinqlx_add_console_command;
    use super::MAIN_ENGINE;
    use crate::commands::cmd_py_command;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn add_console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_add_console_command(py, "slap");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn add_console_command_adds_py_command_to_main_engine() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_add_command()
            .withf(|cmd, &func| cmd == "asdf" && func as usize == cmd_py_command as usize)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| pyshinqlx_add_console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}
