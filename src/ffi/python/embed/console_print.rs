#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_com_printf;
#[cfg(not(test))]
use crate::hooks::shinqlx_com_printf;

use pyo3::{pyfunction, Python};

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
#[cfg(not(miri))]
mod console_print_tests {
    use super::pyshinqlx_console_print;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn console_print_forwards_to_ql_engine() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx.expect().with(predicate::eq("asdf\n"));

        Python::with_gil(|py| {
            pyshinqlx_console_print(py, "asdf");
        });
    }
}
