use crate::MAIN_ENGINE;
use crate::commands::cmd_py_command;
use crate::ffi::python::prelude::*;
use crate::quake_live_engine::AddCommand;

use pyo3::exceptions::PyEnvironmentError;

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
pub(crate) fn pyshinqlx_add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(|| {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| {
                main_engine.add_command(command, cmd_py_command);

                Ok(())
            },
        )
    })
}

#[cfg(test)]
mod add_console_command_tests {
    use super::cmd_py_command;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use rstest::rstest;

    use pyo3::exceptions::PyEnvironmentError;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_console_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_add_console_command(py, "slap");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_console_command_adds_py_command_to_main_engine(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_add_command()
                    .withf(|cmd, &func| cmd == "asdf" && func as usize == cmd_py_command as usize)
                    .times(1);
            })
            .run(|| {
                let result = Python::with_gil(|py| pyshinqlx_add_console_command(py, "asdf"));
                assert!(result.is_ok());
            });
    }
}
