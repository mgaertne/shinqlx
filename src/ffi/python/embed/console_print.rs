use crate::ffi::python::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_com_printf;
#[cfg(not(test))]
use crate::hooks::shinqlx_com_printf;

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
pub(crate) fn pyshinqlx_console_print(py: Python<'_>, text: &str) {
    py.detach(|| {
        let formatted_string = format!("{text}\n");
        shinqlx_com_printf(formatted_string.as_str());
    })
}

#[cfg(test)]
mod console_print_tests {
    use mockall::predicate;
    use rstest::rstest;

    use crate::{
        ffi::python::prelude::*, hooks::mock_hooks::shinqlx_com_printf_context, prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_forwards_to_ql_engine(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx.expect().with(predicate::eq("asdf\n"));

        Python::attach(|py| {
            pyshinqlx_console_print(py, "asdf");
        });
    }
}
