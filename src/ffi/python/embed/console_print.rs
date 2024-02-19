use crate::ffi::python::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_com_printf;
#[cfg(not(test))]
use crate::hooks::shinqlx_com_printf;

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
pub(crate) fn pyshinqlx_console_print(py: Python<'_>, text: &str) {
    py.allow_threads(|| {
        let formatted_string = format!("{}\n", text);
        shinqlx_com_printf(formatted_string.as_str());
    })
}

#[cfg(test)]
mod console_print_tests {
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;

    use mockall::predicate;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_forwards_to_ql_engine() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx.expect().with(predicate::eq("asdf\n"));

        Python::with_gil(|py| {
            pyshinqlx_console_print(py, "asdf");
        });
    }
}
