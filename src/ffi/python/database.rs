use super::pyshinqlx_get_logger;

use pyo3::exceptions::PyNotImplementedError;
use pyo3::prelude::*;

#[pyclass(name = "AbstractDatabase", module = "database", sequence, subclass)]
pub(crate) struct AbstractDatabase {
    plugin: PyObject,
}

#[pymethods]
impl AbstractDatabase {
    #[new]
    fn py_new(_py: Python<'_>, plugin: PyObject) -> Self {
        Self { plugin }
    }

    #[getter(logger)]
    fn get_logger<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let plugin_name = self
            .plugin
            .bind(py)
            .get_type()
            .name()
            .map(|value| value.to_string())?;
        pyshinqlx_get_logger(py, Some(plugin_name.into_py(py)))
    }

    /// Abstract method. Should set the permission of a player.
    #[allow(unused_variables)]
    fn set_permission(&self, player: PyObject, level: i32) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return the permission of a player.
    #[allow(unused_variables)]
    fn get_permission(&self, player: PyObject) -> PyResult<i32> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return whether or not a player has more than or equal
    /// to a certain permission level. Should only take a value of 0 to 5, where 0 is
    /// always True.
    #[allow(unused_variables)]
    #[pyo3(signature = (player, level=5), text_signature = "(player, level=5)")]
    fn has_permission(&self, player: PyObject, level: i32) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should set specified player flag to value.
    #[allow(unused_variables)]
    #[pyo3(signature = (player, flag, value=true), text_signature = "(player, flag, value=true)")]
    fn set_flag(&self, player: PyObject, flag: &str, value: bool) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Should clear specified player flag.
    #[allow(unused_variables)]
    fn clear_flag(&self, player: PyObject, flag: &str) -> PyResult<bool> {
        self.set_flag(player, flag, false)
    }

    /// Abstract method. Should return specified player flag
    #[allow(unused_variables)]
    #[pyo3(signature = (player, flag, default=false), text_signature = "(player, flag, default=false)")]
    fn get_flag(&self, player: PyObject, flag: &str, default: bool) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return a connection to the database. Exactly what a
    /// "connection" obviously depends on the database, so the specifics will be up
    /// to the implementation.
    ///
    /// A :class:`shinqlx.Plugin` subclass can set
    fn connect(&self) -> PyResult<PyObject> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. If the database has a connection state, this method should
    /// close the connection.
    fn close(&self) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }
}
