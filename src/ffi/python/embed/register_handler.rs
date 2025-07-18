use core::hint::cold_path;

use pyo3::exceptions::{PyTypeError, PyValueError};

use crate::ffi::python::prelude::*;

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None), text_signature = "(event, handler=None)")]
pub(crate) fn pyshinqlx_register_handler(
    _py: Python<'_>,
    event: &str,
    handler: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    if handler
        .as_ref()
        .is_some_and(|handler_function| !handler_function.is_callable())
    {
        cold_path();
        return Err(PyTypeError::new_err("The handler must be callable."));
    }

    let handler_lock = match event {
        "custom_command" => &CUSTOM_COMMAND_HANDLER,
        _ => {
            cold_path();
            return Err(PyValueError::new_err("Unsupported event."));
        }
    };

    handler_lock.store(handler.map(|handler_func| handler_func.unbind().into()));
    Ok(())
}

#[cfg(test)]
mod register_handler_tests {
    use pyo3::{
        exceptions::{PyTypeError, PyValueError},
        types::PyBool,
    };
    use rstest::*;

    use crate::{
        ffi::python::{prelude::*, pyshinqlx_test_support::python_function_returning},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_setting_handler_to_none(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let py_handler = python_function_returning(py, &py.None());
            CUSTOM_COMMAND_HANDLER.store(Some(py_handler.unbind().into()));

            let result =
                Python::with_gil(|py| pyshinqlx_register_handler(py, "custom_command", None));
            assert!(result.is_ok());

            let stored_handler = CUSTOM_COMMAND_HANDLER.load();
            assert!(stored_handler.is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_custom_command_handler_setting_handler_to_some_handler(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let py_handler = python_function_returning(py, &py.None());
            CUSTOM_COMMAND_HANDLER.store(None);

            let result = Python::with_gil(|py| {
                pyshinqlx_register_handler(py, "custom_command", Some(py_handler))
            });
            assert!(result.is_ok());

            let stored_handler = CUSTOM_COMMAND_HANDLER.load();
            assert!(stored_handler.is_some());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_for_some_unknown_event(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let py_handler = python_function_returning(py, &py.None());

            let result = pyshinqlx_register_handler(py, "unknown_event", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_for_uncallable_handler(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_register_handler(
                py,
                "custom_command",
                Some(PyBool::new(py, true).to_owned().into_any()),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}
