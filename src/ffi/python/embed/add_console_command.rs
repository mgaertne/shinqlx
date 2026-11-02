use crate::{
    commands::cmd_py_command,
    ffi::python::{prelude::*, with_main_engine},
    quake_live_engine::AddCommand,
};

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
pub(crate) fn pyshinqlx_add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.detach(|| {
        with_main_engine(|main_engine| {
            main_engine.add_command(command, cmd_py_command);

            Ok(())
        })
    })
}

#[cfg(test)]
mod add_console_command_tests {
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    use super::cmd_py_command;
    use crate::{ffi::python::prelude::*, prelude::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_console_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
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
                    .withf(|cmd, &func| {
                        cmd == "asdf" && func as usize == cmd_py_command as *const () as usize
                    })
                    .times(1);
            })
            .run(|| {
                let result = Python::attach(|py| pyshinqlx_add_console_command(py, "asdf"));
                assert!(result.is_ok());
            });
    }
}
