use core::{cmp::max, hint::cold_path};

use itertools::Itertools;
use pyo3::{
    IntoPyObjectExt, PyTraverseError, PyVisit,
    exceptions::{
        PyEnvironmentError, PyKeyError, PyNotImplementedError, PyRuntimeError, PyValueError,
    },
    intern,
    prelude::*,
    types::{IntoPyDict, PyBool, PyDict, PyInt, PyString, PyTuple},
};

use super::{owner, prelude::*, pyshinqlx_get_logger};
use crate::{MAIN_ENGINE, quake_live_engine::FindCVar};

#[pyclass(name = "AbstractDatabase", module = "database", subclass, frozen)]
pub(crate) struct AbstractDatabase {
    plugin: Py<PyAny>,
}

#[pymethods]
impl AbstractDatabase {
    #[new]
    fn py_new(_py: Python<'_>, plugin: &Bound<'_, PyAny>) -> Self {
        Self {
            plugin: plugin.to_owned().unbind(),
        }
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.plugin)?;
        Ok(())
    }

    fn __clear__(&self) {}

    #[getter(logger)]
    fn get_logger<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_logger()
    }

    /// Abstract method. Should set the permission of a player.
    fn set_permission(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<()> {
        slf.set_permission(player, level)
    }

    /// Abstract method. Should return the permission of a player.
    fn get_permission(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<i32> {
        slf.get_permission(player)
    }

    /// Abstract method. Should return whether or not a player has more than or equal
    /// to a certain permission level. Should only take a value of 0 to 5, where 0 is
    /// always True.
    #[pyo3(signature = (player, level=5), text_signature = "(player, level=5)")]
    fn has_permission(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<bool> {
        slf.has_permission(player, level)
    }

    /// Abstract method. Should set specified player flag to value.
    #[pyo3(signature = (player, flag, value=true), text_signature = "(player, flag, value=true)")]
    fn set_flag(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        flag: &str,
        value: bool,
    ) -> PyResult<()> {
        slf.set_flag(player, flag, value)
    }

    /// Should clear specified player flag.
    fn clear_flag(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>, flag: &str) -> PyResult<()> {
        slf.clear_flag(player, flag)
    }

    /// Abstract method. Should return specified player flag
    #[pyo3(signature = (player, flag, default=false), text_signature = "(player, flag, default=false)")]
    fn get_flag(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        flag: &str,
        default: bool,
    ) -> PyResult<bool> {
        slf.get_flag(player, flag, default)
    }

    /// Abstract method. Should return a connection to the database. Exactly what a
    /// "connection" obviously depends on the database, so the specifics will be up
    /// to the implementation.
    ///
    /// A :class:`shinqlx.Plugin` subclass can set
    fn connect<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.connect()
    }

    /// Abstract method. If the database has a connection state, this method should
    /// close the connection.
    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.close()
    }
}

pub(crate) trait AbstractDatabaseMethods<'py> {
    fn get_logger(&self) -> PyResult<Bound<'py, PyAny>>;
    fn set_permission(&self, player: &Bound<'py, PyAny>, level: i32) -> PyResult<()>;
    fn get_permission(&self, player: &Bound<'py, PyAny>) -> PyResult<i32>;
    fn has_permission(&self, player: &Bound<'py, PyAny>, level: i32) -> PyResult<bool>;
    fn set_flag(&self, player: &Bound<'py, PyAny>, flag: &str, value: bool) -> PyResult<()>;
    fn clear_flag(&self, player: &Bound<'py, PyAny>, flag: &str) -> PyResult<()>;
    fn get_flag(&self, player: &Bound<'py, PyAny>, flag: &str, default: bool) -> PyResult<bool>;
    fn connect(&self) -> PyResult<Bound<'py, PyAny>>;
    fn close(&self) -> PyResult<()>;
}

impl<'py> AbstractDatabaseMethods<'py> for Bound<'py, AbstractDatabase> {
    fn get_logger(&self) -> PyResult<Bound<'py, PyAny>> {
        let bound_plugin = self.get().plugin.bind(self.py()).to_owned();
        pyshinqlx_get_logger(self.py(), Some(bound_plugin))
    }

    #[allow(unused_variables)]
    fn set_permission(&self, player: &Bound<'_, PyAny>, level: i32) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    #[allow(unused_variables)]
    fn get_permission(&self, player: &Bound<'_, PyAny>) -> PyResult<i32> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    #[allow(unused_variables)]
    fn has_permission(&self, player: &Bound<'_, PyAny>, level: i32) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    #[allow(unused_variables)]
    fn set_flag(&self, player: &Bound<'_, PyAny>, flag: &str, value: bool) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    fn clear_flag(&self, player: &Bound<'_, PyAny>, flag: &str) -> PyResult<()> {
        self.set_flag(player, flag, false)
    }

    #[allow(unused_variables)]
    fn get_flag(&self, player: &Bound<'_, PyAny>, flag: &str, default: bool) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    fn connect(&self) -> PyResult<Bound<'py, PyAny>> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    fn close(&self) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }
}

#[cfg(test)]
mod abstract_database_tests {
    use pyo3::{exceptions::PyNotImplementedError, intern, prelude::*};
    use rstest::rstest;

    use super::{
        super::{
            prelude::pyshinqlx_setup,
            pyshinqlx_test_support::{default_test_player, test_plugin},
        },
        AbstractDatabase, AbstractDatabaseMethods,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_logger_returns_logger_for_plugin(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let test_plugin = test_plugin(py).call0().expect("this should not happen");
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, test_plugin.as_any()))
                    .expect("this should not happen");

            let result = abstract_database.getattr(intern!(py, "logger"));
            assert!(result.is_ok_and(|logger| {
                logger
                    .getattr(intern!(py, "name"))
                    .expect("this should not happen")
                    .to_string()
                    == "shinqlx.test_plugin"
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn set_permission_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.set_permission(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                42,
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_permission_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.get_permission(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn has_permission_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.has_permission(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                42,
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn set_flag_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.set_flag(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                "asdf",
                true,
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn clear_flag_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.clear_flag(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                "asdf",
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_flag_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.get_flag(
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                "asdf",
                true,
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn connect_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.connect();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn close_returns_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_database =
                Bound::new(py, AbstractDatabase::py_new(py, py.None().bind(py)))
                    .expect("this should not happen");

            let result = abstract_database.close();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)))
        });
    }

    fn python_test_db(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code(
            py,
            cr#"
from shinqlx import Plugin
from shinqlx.database import AbstractDatabase

class test_plugin(Plugin):
    pass

db = AbstractDatabase(test_plugin())
            "#,
            c"",
            c"",
        )
        .expect("this should not happen")
        .getattr(intern!(py, "db"))
        .expect("this should not happen")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_logger_returns_logger_for_plugin_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.getattr(intern!(py, "logger"));
            assert!(result.is_ok_and(|logger| {
                logger
                    .getattr(intern!(py, "name"))
                    .expect("this should not happen")
                    .to_string()
                    == "shinqlx.test_plugin"
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn set_permission_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "set_permission"), (py.None(), 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_permission_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "get_permission"), (py.None(),));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn has_permission_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "has_permission"), (py.None(),));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn set_flag_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "set_flag"), (py.None(), "asdf"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn clear_flag_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "clear_flag"), (py.None(), "asdf"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_flag_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method1(intern!(py, "get_flag"), (py.None(), "asdf"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn connect_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method0(intern!(py, "connect"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn close_returns_not_implemented_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let db = python_test_db(py);

            let result = db.call_method0(intern!(py, "close"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }
}

/// A subclass of :class:`shinqlx.AbstractDatabase` providing support for Redis.
#[pyclass(name = "Redis", module = "database", extends = AbstractDatabase, dict, frozen)]
pub(crate) struct Redis {}

#[pymethods]
impl Redis {
    #[new]
    fn py_new(py: Python<'_>, plugin: &Bound<'_, PyAny>) -> (Self, AbstractDatabase) {
        let redis_type = py.get_type::<Self>();
        let counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        let _ = redis_type.setattr(intern!(py, "_counter"), counter + 1);

        (
            Self {},
            AbstractDatabase {
                plugin: plugin.to_owned().unbind(),
            },
        )
    }

    fn __del__(slf_: &Bound<'_, Self>) -> PyResult<()> {
        Self::close(slf_)?;
        let redis_type = slf_.py().get_type::<Redis>();
        let counter = redis_type
            .getattr(intern!(slf_.py(), "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        redis_type.setattr(intern!(slf_.py(), "_counter"), max(0, counter - 1))?;

        Ok(())
    }

    #[getter(r)]
    fn get_redis<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_redis()
    }

    fn __contains__(slf_: &Bound<'_, Self>, key: &str) -> PyResult<bool> {
        let redis_connection = Self::get_redis(slf_)?;
        redis_connection
            .call_method1(intern!(slf_.py(), "exists"), (key,))
            .map(|value| value.to_string() != "0")
    }

    fn __getitem__<'py>(slf_: &Bound<'py, Self>, key: &str) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = Self::get_redis(slf_)?;
        redis_connection
            .call_method1(intern!(slf_.py(), "get"), (key,))
            .and_then(|value| {
                if value.is_none() {
                    let error_msg = format!("The key '{key}' is not present in the database.");
                    Err(PyKeyError::new_err(error_msg))
                } else {
                    Ok(value)
                }
            })
    }

    fn __setitem__(slf_: &Bound<'_, Self>, key: &str, item: &Bound<'_, PyAny>) -> PyResult<()> {
        let redis_connection = Self::get_redis(slf_)?;
        let returned = redis_connection
            .call_method1(intern!(slf_.py(), "set"), (key, item))
            .and_then(|value| value.extract::<bool>())?;

        if !returned {
            cold_path();
            return Err(PyRuntimeError::new_err("The database assignment failed."));
        }

        Ok(())
    }

    fn __delitem__(slf_: &Bound<'_, Self>, key: &str) -> PyResult<()> {
        let redis_connection = Self::get_redis(slf_)?;
        let returned = redis_connection
            .call_method1(intern!(slf_.py(), "delete"), (key,))
            .and_then(|value| value.extract::<i32>())?;

        if returned == 0 {
            let error_msg = format!("The key '{key}' is not present in the database.");
            return Err(PyKeyError::new_err(error_msg));
        }

        Ok(())
    }

    fn __getattr__<'py>(slf_: &Bound<'py, Self>, attr: &str) -> PyResult<Bound<'py, PyAny>> {
        if ["_conn", "_pool"].contains(&attr) {
            return Ok(slf_.py().None().into_bound(slf_.py()));
        }
        let redis_connection = Self::get_redis(slf_)?;
        redis_connection.getattr(attr)
    }

    /// Sets the permission of a player.
    #[pyo3(name = "set_permission")]
    fn set_permission(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<()> {
        slf.set_permission(player, level)
    }

    /// Gets the permission of a player.
    fn get_permission(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<i32> {
        slf.get_permission(player)
    }

    /// Checks if the player has higher than or equal to *level*.
    #[pyo3(name = "has_permission", signature = (player, level = 5), text_signature = "(player, level=5)")]
    fn has_permission(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<bool> {
        slf.has_permission(player, level)
    }

    /// Sets specified player flag
    #[pyo3(name = "set_flag", signature = (player, flag, value = true), text_signature = "(player, flag, value = True)")]
    fn set_flag(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        flag: &str,
        value: bool,
    ) -> PyResult<()> {
        slf.set_flag(player, flag, value)
    }

    /// returns the specified player flag
    #[pyo3(name = "get_flag", signature = (player, flag, default = false), text_signature = "(player, flag, default=False)")]
    fn get_flag(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        flag: &str,
        default: bool,
    ) -> PyResult<bool> {
        slf.get_flag(player, flag, default)
    }

    /// Returns a connection to a Redis database. If *host* is None, it will
    /// fall back to the settings in the config and ignore the rest of the arguments.
    /// It will also share the connection across any plugins using the default
    /// configuration. Passing *host* will make it connect to a specific database
    /// that is not shared at all. Subsequent calls to this will return the connection
    /// initialized the first call unless it has been closed.
    #[pyo3(name = "connect", signature = (host = None, database = 0, unix_socket = false, password = None), text_signature = "(host = None, database = 0, unix_socket = false, password = None)")]
    fn connect<'py>(
        slf: &Bound<'py, Self>,
        host: Option<&str>,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        match host {
            None => slf.connect(),
            Some(hostname) => {
                slf.connect_with_parameters(hostname, database, unix_socket, password)
            }
        }
    }

    /// Close the Redis connection if the config was overridden. Otherwise only do so
    /// if this is the last plugin using the default connection.
    fn close(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.close()
    }

    #[pyo3(name = "mset", signature = (*args, **kwargs))]
    fn mset<'py>(
        slf: &Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.mset(args, kwargs)
    }

    #[pyo3(name = "msetnx", signature = (*args, **kwargs))]
    fn msetnx<'py>(
        slf: &Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.msetnx(args, kwargs)
    }

    #[pyo3(name = "zadd", signature = (name, *args, **kwargs))]
    fn zadd<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.zadd(name, args, kwargs)
    }

    #[pyo3(name = "zincrby", signature = (name, *, value, amount), text_signature = "(name, *, value, amount)")]
    fn zincrby<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        amount: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.zincrby(name, value, amount)
    }

    #[pyo3(name = "setx", signature = (name, *, value, time), text_signature = "(name, *, value, time)")]
    fn setx<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        time: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.setx(name, value, time)
    }

    #[pyo3(name = "lrem", signature = (name, *, value, count), text_signature = "(name, *, value, count)")]
    fn lrem<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.lrem(name, value, count)
    }
}

impl<'py> AbstractDatabaseMethods<'py> for Bound<'py, Redis> {
    fn get_logger(&self) -> PyResult<Bound<'py, PyAny>> {
        self.as_super().get_logger()
    }

    fn set_permission(&self, player: &Bound<'py, PyAny>, level: i32) -> PyResult<()> {
        let key = match player.extract::<Player>() {
            Ok(rust_player) => format!("minqlx:players:{}:permission", rust_player.steam_id),
            _ => format!("minqlx:players:{}:permission", player.str()?),
        };

        self.set_item(&key, PyInt::new(self.py(), level))
    }

    fn get_permission(&self, player: &Bound<'py, PyAny>) -> PyResult<i32> {
        let steam_id = match player.extract::<Player>() {
            Ok(rust_player) => Ok(rust_player.steam_id),
            _ => match player.extract::<i64>() {
                Ok(steam_id) => Ok(steam_id),
                _ => player
                    .extract::<String>()
                    .and_then(|rust_str| {
                        rust_str.parse::<i64>().map_err(|_| {
                            let error_msg =
                                format!("invalid literal for int() with base 10: '{rust_str}'");
                            PyValueError::new_err(error_msg)
                        })
                    })
                    .map_err(|_| {
                        PyValueError::new_err(
                            "Invalid player. Use either a shinqlx.Player instance or a SteamID64.",
                        )
                    }),
            },
        }?;

        if Some(steam_id) == owner()? {
            return Ok(5);
        }

        let key = format!("minqlx:players:{steam_id}:permission");
        if !self.contains(&key)? {
            return Ok(0);
        }
        self.get_item(&key).and_then(|value| {
            value.to_string().parse::<i32>().map_err(|_| {
                let error_msg = format!("invalid literal for int() with base 10: '{value}",);
                PyValueError::new_err(error_msg)
            })
        })
    }

    fn has_permission(&self, player: &Bound<'py, PyAny>, level: i32) -> PyResult<bool> {
        self.get_permission(player).map(|value| value >= level)
    }

    fn set_flag(&self, player: &Bound<'py, PyAny>, flag: &str, value: bool) -> PyResult<()> {
        let key = match player.extract::<Player>() {
            Ok(rust_player) => format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag),
            _ => format!("minqlx:players:{}:flags:{}", player.str()?, flag),
        };

        let redis_value = if value { 1i32 } else { 0i32 };

        self.set_item(&key, PyInt::new(self.py(), redis_value))
    }

    fn clear_flag(&self, player: &Bound<'py, PyAny>, flag: &str) -> PyResult<()> {
        self.set_flag(player, flag, false)
    }

    fn get_flag(&self, player: &Bound<'py, PyAny>, flag: &str, default: bool) -> PyResult<bool> {
        let key = match player.extract::<Player>() {
            Ok(rust_player) => format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag),
            _ => format!("minqlx:players:{}:flags:{}", player.str()?, flag),
        };

        if !self.contains(&key)? {
            return Ok(default);
        }

        self.get_item(&key).map(|value| value.to_string() != "0")
    }

    fn connect(&self) -> PyResult<Bound<'py, PyAny>> {
        match self.getattr(intern!(self.py(), "_conn")) {
            Ok(redis_connection) if !redis_connection.is_none() => Ok(redis_connection),
            _ => {
                match self
                    .py()
                    .get_type::<Redis>()
                    .getattr(intern!(self.py(), "_conn"))
                {
                    Ok(class_connection) if !class_connection.is_none() => Ok(class_connection),
                    _ => {
                        let py_redis = self.py().import(intern!(self.py(), "redis"))?;
                        let strict_redis = py_redis.getattr(intern!(self.py(), "StrictRedis"))?;

                        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                            cold_path();
                            return Err(PyEnvironmentError::new_err(
                                "could not get access to main engine.",
                            ));
                        };

                        let Some(cvar_host) = main_engine.find_cvar("qlx_redisAddress") else {
                            cold_path();
                            return Err(PyValueError::new_err(
                                "cvar qlx_redisAddress misconfigured",
                            ));
                        };
                        let Some(redis_db_cvar) = main_engine
                            .find_cvar("qlx_redisDatabase")
                            .and_then(|cvar| cvar.get_string().parse::<i64>().ok())
                        else {
                            cold_path();
                            return Err(PyValueError::new_err(
                                "cvar qlx_redisDatabase misconfigured.",
                            ));
                        };
                        let Some(unix_socket_cvar) =
                            main_engine.find_cvar("qlx_redisUnixSocket").map(|cvar| {
                                let cvar_string = cvar.get_string();
                                !cvar_string.is_empty() && cvar_string != "0"
                            })
                        else {
                            cold_path();
                            return Err(PyValueError::new_err(
                                "cvar qlx_redisUnixSocket misconfigured.",
                            ));
                        };
                        let Some(password_cvar) = main_engine.find_cvar("qlx_redisPassword") else {
                            cold_path();
                            return Err(PyValueError::new_err(
                                "cvar qlx_redisPassword misconfigured.",
                            ));
                        };

                        let class_connection = if unix_socket_cvar {
                            strict_redis.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        (
                                            "unix_socket_path",
                                            PyString::intern(self.py(), &cvar_host.get_string())
                                                .into_any(),
                                        ),
                                        ("db", PyInt::new(self.py(), redis_db_cvar).into_any()),
                                        (
                                            "password",
                                            PyString::intern(
                                                self.py(),
                                                &password_cvar.get_string(),
                                            )
                                            .into_any(),
                                        ),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                        } else {
                            let hostname = cvar_host.get_string();
                            let (redis_hostname, port) = hostname
                                .split_once(':')
                                .unwrap_or((hostname.as_ref(), "6379"));
                            let redis_port = if port.is_empty() { "6379" } else { port };
                            let connection_pool =
                                py_redis.getattr(intern!(self.py(), "ConnectionPool"))?;

                            let redis_pool = connection_pool.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        (
                                            "host",
                                            PyString::intern(self.py(), redis_hostname).into_any(),
                                        ),
                                        (
                                            "port",
                                            PyString::intern(self.py(), redis_port).into_any(),
                                        ),
                                        ("db", PyInt::new(self.py(), redis_db_cvar).into_any()),
                                        (
                                            "password",
                                            PyString::intern(
                                                self.py(),
                                                &password_cvar.get_string(),
                                            )
                                            .into_any(),
                                        ),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )?;
                            self.py()
                                .get_type::<Redis>()
                                .setattr(intern!(self.py(), "_pool"), &redis_pool)?;
                            strict_redis.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        ("connection_pool", redis_pool),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                        }?;
                        self.py()
                            .get_type::<Redis>()
                            .setattr(intern!(self.py(), "_conn"), &class_connection)?;
                        self.setattr(intern!(self.py(), "_conn"), self.py().None())?;
                        Ok(class_connection)
                    }
                }
            }
        }
    }

    fn close(&self) -> PyResult<()> {
        match self.getattr(intern!(self.py(), "_conn")) {
            Ok(instance_connection) if !instance_connection.is_none() => {
                self.setattr(intern!(self.py(), "_conn"), self.py().None())?;
                match self.getattr(intern!(self.py(), "_pool")) {
                    Ok(instance_pool) if !instance_pool.is_none() => {
                        instance_pool.call_method0(intern!(self.py(), "disconnect"))?;
                        self.setattr(intern!(self.py(), "_pool"), self.py().None())?;
                    }
                    _ => (),
                }
            }
            _ => (),
        }

        let redis_type = self.py().get_type::<Redis>();
        let class_counter = redis_type
            .getattr(intern!(self.py(), "_counter"))
            .and_then(|value| value.extract::<i32>())
            .unwrap_or(0);
        if class_counter <= 1
            && redis_type
                .getattr(intern!(self.py(), "_conn"))
                .is_ok_and(|class_connection| !class_connection.is_none())
        {
            redis_type.setattr(intern!(self.py(), "_conn"), self.py().None())?;
            match redis_type.getattr(intern!(self.py(), "_pool")) {
                Ok(class_pool) if !class_pool.is_none() => {
                    class_pool.call_method0(intern!(self.py(), "disconnect"))?;
                    redis_type.setattr(intern!(self.py(), "_pool"), self.py().None())?;
                }
                _ => (),
            }
        }
        Ok(())
    }
}

pub(crate) trait RedisMethods<'py> {
    fn get_redis(&self) -> PyResult<Bound<'py, PyAny>>;
    fn connect_with_parameters(
        &self,
        host: &str,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn mset(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn msetnx(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn zadd(
        &self,
        name: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn zincrby(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        amount: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn setx(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        time: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn lrem(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
}

impl<'py> RedisMethods<'py> for Bound<'py, Redis> {
    fn get_redis(&self) -> PyResult<Bound<'py, PyAny>> {
        self.connect()
    }

    fn connect_with_parameters(
        &self,
        host: &str,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        match self.getattr(intern!(self.py(), "_conn")) {
            Ok(redis_connection) if !redis_connection.is_none() => Ok(redis_connection),
            _ => {
                match self
                    .py()
                    .get_type::<Redis>()
                    .getattr(intern!(self.py(), "_conn"))
                {
                    Ok(class_connection) if !class_connection.is_none() => Ok(class_connection),
                    _ => {
                        let py_redis = self.py().import(intern!(self.py(), "redis"))?;
                        let strict_redis = py_redis.getattr(intern!(self.py(), "StrictRedis"))?;
                        let instance_connection = if unix_socket {
                            strict_redis.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        (
                                            "unix_socket_path",
                                            PyString::intern(self.py(), host).into_any(),
                                        ),
                                        ("db", PyInt::new(self.py(), database).into_any()),
                                        ("password", password.into_bound_py_any(self.py())?),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                        } else {
                            let (redis_hostname, port) =
                                host.split_once(':').unwrap_or((host, "6379"));
                            let redis_port = if port.is_empty() { "6379" } else { port };
                            let connection_pool =
                                py_redis.getattr(intern!(self.py(), "ConnectionPool"))?;

                            let redis_pool = connection_pool.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        (
                                            "host",
                                            PyString::intern(self.py(), redis_hostname).into_any(),
                                        ),
                                        (
                                            "port",
                                            PyString::intern(self.py(), redis_port).into_any(),
                                        ),
                                        ("db", PyInt::new(self.py(), database).into_any()),
                                        ("password", password.into_bound_py_any(self.py())?),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )?;
                            self.setattr(intern!(self.py(), "_pool"), &redis_pool)?;
                            strict_redis.call(
                                PyTuple::empty(self.py()),
                                Some(
                                    &[
                                        ("connection_pool", redis_pool),
                                        (
                                            "decode_responses",
                                            PyBool::new(self.py(), true).to_owned().into_any(),
                                        ),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                        }?;
                        self.setattr(intern!(self.py(), "_conn"), &instance_connection)?;
                        Ok(instance_connection)
                    }
                }
            }
        }
    }

    fn mset(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_module = self.py().import(intern!(self.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(self.py(), "RedisError"))?;

        let mapping = PyDict::new(self.py());

        if args.len() > 1 {
            cold_path();
            let error = redis_error.call1((intern!(
                self.py(),
                "MSET requires **kwargs or a single dict arg"
            ),))?;
            return Err(PyErr::from_value(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0) else {
                cold_path();
                let error = redis_error.call1((intern!(
                    self.py(),
                    "MSET requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.cast::<PyDict>()?.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = self.get_redis()?;
        redis_connection.call_method1(intern!(self.py(), "mset"), (mapping,))
    }

    fn msetnx(
        &self,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_module = self.py().import(intern!(self.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(self.py(), "RedisError"))?;

        let mapping = PyDict::new(self.py());

        if args.len() > 1 {
            cold_path();
            let error = redis_error.call1((intern!(
                self.py(),
                "MSENXT requires **kwargs or a single dict arg"
            ),))?;
            return Err(PyErr::from_value(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0) else {
                cold_path();
                let error = redis_error.call1((intern!(
                    self.py(),
                    "MSETNX requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.cast::<PyDict>()?.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = Self::get_redis(self)?;
        redis_connection.call_method1(intern!(self.py(), "msetnx"), (mapping,))
    }

    fn zadd(
        &self,
        name: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        if args.len() == 1 && args.get_item(0)?.is_instance_of::<PyDict>() {
            let args_tuple = PyTuple::new(self.py(), [name].iter())?
                .as_sequence()
                .concat(args.as_sequence())?
                .to_tuple()?;
            return redis_connection.call_method(intern!(self.py(), "zadd"), args_tuple, kwargs);
        }

        let redis_module = self.py().import(intern!(self.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(self.py(), "RedisError"))?;

        if args.len() % 2 != 0 {
            cold_path();
            let error = redis_error.call1((intern!(
                self.py(),
                "ZADD requires an equal number of values and scores"
            ),))?;
            return Err(PyErr::from_value(error));
        }
        let pieces = args
            .iter()
            .map(|item| item.to_string())
            .tuples()
            .map(|(a, b)| (b, a))
            .collect::<Vec<_>>();

        redis_connection.call_method(
            intern!(self.py(), "zadd"),
            (name, pieces.into_py_dict(self.py())?),
            kwargs,
        )
    }

    fn zincrby(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        amount: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        redis_connection.call_method1(intern!(self.py(), "zincrby"), (name, amount, value))
    }

    fn setx(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        time: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        redis_connection.call_method1(intern!(self.py(), "setx"), (name, value, time))
    }

    fn lrem(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        redis_connection.call_method1(intern!(self.py(), "lrem"), (name, value, count))
    }
}
