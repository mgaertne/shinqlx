use core::{cmp::max, hint::cold_path};

use itertools::Itertools;
use pyo3::{
    IntoPyObjectExt, PyTraverseError, PyVisit,
    exceptions::{
        PyEnvironmentError, PyKeyError, PyNotImplementedError, PyRuntimeError, PyValueError,
    },
    intern,
    prelude::*,
    types::{IntoPyDict, PyBool, PyDict, PyInt, PyNone, PyString, PyTuple},
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
    #[pyo3(signature = (player, flag, value=true), text_signature = "(player, flag, value=True)")]
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
    #[pyo3(signature = (player, flag, default=false), text_signature = "(player, flag, default=False)")]
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
    fn py_new(py: Python<'_>, plugin: &Bound<'_, PyAny>) -> PyClassInitializer<Self> {
        let redis_type = py.get_type::<Self>();
        let counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        let _ = redis_type.setattr(intern!(py, "_counter"), counter + 1);

        PyClassInitializer::from(AbstractDatabase {
            plugin: plugin.to_owned().unbind(),
        })
        .add_subclass(Self {})
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
    #[pyo3(name = "connect", signature = (host = None, database = 0, unix_socket = false, password = None), text_signature = "(host = None, database = 0, unix_socket = False, password = None)")]
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

    #[pyo3(name = "zincrby", signature = (name, *, value, amount = None), text_signature = "(name, *, value, amount = 1)")]
    fn zincrby<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        amount: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let default_amount = PyInt::new(slf.py(), 1).into_any();
        slf.zincrby(name, value, amount.unwrap_or(&default_amount))
    }

    #[pyo3(name = "setex", signature = (name, *, value, time), text_signature = "(name, *, value, time)")]
    fn setex<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        time: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.setex(name, value, time)
    }

    #[pyo3(name = "lrem", signature = (name, value, count = 0), text_signature = "(name, value, count = 0)")]
    fn lrem<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        value: &Bound<'py, PyAny>,
        count: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.lrem(name, value, &PyInt::new(slf.py(), count).into_any())
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
                let error_msg = format!("invalid literal for int() with base 10: '{value}'",);
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
                        let py_redis_module = self.py().import(intern!(self.py(), "redis"))?;
                        let py_redis_client =
                            py_redis_module.getattr(intern!(self.py(), "Redis"))?;

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
                            py_redis_client.call(
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
                                        (
                                            "socket_timeout",
                                            PyNone::get(self.py()).to_owned().into_any(),
                                        ),
                                        ("protocol", PyInt::new(self.py(), 2).into_any()),
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
                                py_redis_module.getattr(intern!(self.py(), "ConnectionPool"))?;

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
                            py_redis_client.call(
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
    fn setex(
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
                        let py_redis_module = self.py().import(intern!(self.py(), "redis"))?;
                        let py_redis_client =
                            py_redis_module.getattr(intern!(self.py(), "Redis"))?;
                        let instance_connection = if unix_socket {
                            py_redis_client.call(
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
                                        (
                                            "socket_timeout",
                                            PyNone::get(self.py()).to_owned().into_any(),
                                        ),
                                        ("protocol", PyInt::new(self.py(), 2).into_any()),
                                    ]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                        } else {
                            let (redis_hostname, port) =
                                host.split_once(':').unwrap_or((host, "6379"));
                            let redis_port = if port.is_empty() { "6379" } else { port };
                            let connection_pool =
                                py_redis_module.getattr(intern!(self.py(), "ConnectionPool"))?;

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
                            py_redis_client.call(
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
            let Some(dict_arg) = args
                .get_item(0)
                .ok()
                .and_then(|item| item.cast_into::<PyDict>().ok())
            else {
                cold_path();
                let error = redis_error.call1((intern!(
                    self.py(),
                    "MSET requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.as_mapping())?;
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
                "MSETNX requires **kwargs or a single dict arg"
            ),))?;
            return Err(PyErr::from_value(error));
        }

        if args.len() == 1 {
            let Some(dict_arg) = args
                .get_item(0)
                .ok()
                .and_then(|item| item.cast_into::<PyDict>().ok())
            else {
                cold_path();
                let error = redis_error.call1((intern!(
                    self.py(),
                    "MSETNX requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.as_mapping())?;
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

        redis_connection.call_method(
            intern!(self.py(), "zincrby"),
            (name,),
            Some(&[("value", value), ("amount", amount)].into_py_dict(self.py())?),
        )
    }

    fn setex(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        time: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        redis_connection.call_method(
            intern!(self.py(), "setex"),
            (name,),
            Some(&[("value", value), ("time", time)].into_py_dict(self.py())?),
        )
    }

    fn lrem(
        &self,
        name: &str,
        value: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = self.get_redis()?;

        redis_connection.call_method(
            intern!(self.py(), "lrem"),
            (name,),
            Some(&[("value", value), ("count", count)].into_py_dict(self.py())?),
        )
    }
}

#[cfg(test)]
mod redis_tests {
    use core::{borrow::BorrowMut, ffi::CStr};

    use pyo3::{
        exceptions::{PyEnvironmentError, PyKeyError, PyRuntimeError, PyValueError},
        intern,
        prelude::*,
        types::{PyDict, PyInt, PyString, PyTuple},
    };
    use rstest::rstest;

    use super::{
        super::{
            prelude::pyshinqlx_setup,
            pyshinqlx_test_support::{default_test_player, test_plugin},
        },
        AbstractDatabaseMethods, Redis, RedisMethods,
    };
    use crate::{
        ffi::c::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
    };

    const FAKE_REDIS: &CStr = cr#"
class RedisError(Exception):
    pass


class ConnectionPool:
    def __init__(self, **kwargs):
        self.kwargs = kwargs
        self.disconnects = 0

    def disconnect(self):
        self.disconnects += 1


class Redis:
    def __init__(self, **kwargs):
        self.kwargs = kwargs
        self.data = {}
        self.calls = []

    def exists(self, key):
        return 1 if key in self.data else 0

    def get(self, key):
        return self.data.get(key)

    def set(self, key, value):
        if key == "fail":
            return False
        self.data[key] = str(value)
        return True

    def delete(self, key):
        return 1 if self.data.pop(key, None) is not None else 0

    def mset(self, mapping):
        self.data.update({key: str(value) for key, value in mapping.items()})
        return True

    def msetnx(self, mapping):
        if any(key in self.data for key in mapping):
            return False
        return self.mset(mapping)

    def zadd(self, name, mapping, **kwargs):
        self.calls.append(("zadd", name, dict(mapping), kwargs))
        return len(mapping)

    def zincrby(self, name, amount, value):
        self.calls.append(("zincrby", name, amount, value))
        return float(amount)

    def setex(self, name, time, value):
        self.calls.append(("setex", name, time, value))
        return True

    def lrem(self, name, count, value):
        self.calls.append(("lrem", name, count, value))
        return 1
"#;

    struct FakeRedis<'py> {
        modules: Bound<'py, PyDict>,
        previous: Option<Bound<'py, PyAny>>,
    }

    fn reset_redis_class_state(py: Python<'_>) {
        let redis_type = py.get_type::<Redis>();
        for attribute in ["_conn", "_pool", "_counter"] {
            let _ = redis_type.delattr(attribute);
        }
    }

    impl<'py> FakeRedis<'py> {
        fn install(py: Python<'py>) -> Self {
            let modules = py
                .import(intern!(py, "sys"))
                .and_then(|sys| sys.getattr(intern!(py, "modules")))
                .and_then(|modules| Ok(modules.cast_into::<PyDict>()?))
                .expect("could not get sys.modules");
            let previous = modules
                .get_item("redis")
                .expect("could not look up the redis module");
            PyModule::from_code(py, FAKE_REDIS, c"fake_redis.py", c"redis")
                .expect("could not create the fake redis module");
            reset_redis_class_state(py);

            Self { modules, previous }
        }
    }

    impl Drop for FakeRedis<'_> {
        fn drop(&mut self) {
            reset_redis_class_state(self.modules.py());
            let _ = match &self.previous {
                Some(previous) => self.modules.set_item("redis", previous),
                None => self.modules.del_item("redis"),
            };
        }
    }

    #[derive(Debug, Clone)]
    struct RedisCvars {
        address: Option<&'static CStr>,
        database: Option<&'static CStr>,
        unix_socket: Option<&'static CStr>,
        password: Option<&'static CStr>,
    }

    fn default_cvars() -> RedisCvars {
        RedisCvars {
            address: Some(c"localhost:6380"),
            database: Some(c"3"),
            unix_socket: Some(c"0"),
            password: Some(c"secret"),
        }
    }

    fn with_cvar(
        engine: MockEngineBuilder,
        name: &'static str,
        value: Option<&'static CStr>,
    ) -> MockEngineBuilder {
        let Some(value) = value else {
            return engine.with_find_cvar(move |cmd| cmd == name, |_| None, 0..);
        };
        let mut raw_cvar = CVarBuilder::default()
            .string(value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        engine.with_find_cvar(
            move |cmd| cmd == name,
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            0..,
        )
    }

    fn redis_engine(cvars: RedisCvars) -> MockEngineBuilder {
        let engine = with_cvar(
            MockEngineBuilder::default(),
            "qlx_redisAddress",
            cvars.address,
        );
        let engine = with_cvar(engine, "qlx_redisDatabase", cvars.database);
        let engine = with_cvar(engine, "qlx_redisUnixSocket", cvars.unix_socket);
        with_cvar(engine, "qlx_redisPassword", cvars.password)
    }

    fn owner_engine(owner: &'static CStr) -> MockEngineBuilder {
        with_cvar(MockEngineBuilder::default(), "qlx_owner", Some(owner))
    }

    fn with_fake_redis<F>(test: F)
    where
        F: for<'py> FnOnce(Python<'py>, Bound<'py, Redis>),
    {
        Python::attach(|py| {
            let _fake = FakeRedis::install(py);
            let plugin = test_plugin(py)
                .call0()
                .expect("could not create the plugin");
            let redis = Bound::new(py, Redis::py_new(py, plugin.as_any()))
                .expect("could not create the database");
            test(py, redis);
        });
    }

    fn with_redis<F>(mut engine: MockEngineBuilder, test: F)
    where
        F: for<'py> FnOnce(Python<'py>, Bound<'py, Redis>),
    {
        engine.run(|| with_fake_redis(test));
    }

    fn connected<'py>(redis: &Bound<'py, Redis>) -> Bound<'py, PyAny> {
        redis
            .connect_with_parameters("localhost", 0, false, None)
            .expect("could not connect")
    }

    fn kwarg(object: &Bound<'_, PyAny>, name: &str) -> String {
        object
            .getattr("kwargs")
            .and_then(|kwargs| kwargs.get_item(name))
            .map(|value| value.to_string())
            .expect("missing keyword argument")
    }

    fn stored(connection: &Bound<'_, PyAny>, key: &str) -> Option<String> {
        connection
            .getattr("data")
            .and_then(|data| data.get_item(key))
            .map(|value| value.to_string())
            .ok()
    }

    fn calls(connection: &Bound<'_, PyAny>) -> String {
        connection
            .getattr("calls")
            .map(|calls| calls.to_string())
            .expect("could not get calls")
    }

    fn error_details(py: Python<'_>, err: &PyErr) -> (String, String) {
        (
            err.get_type(py)
                .name()
                .map(|name| name.to_string())
                .unwrap_or_default(),
            err.value(py).to_string(),
        )
    }

    #[rstest]
    #[case(c"localhost:6380", "localhost", "6380")]
    #[case(c"localhost", "localhost", "6379")]
    #[case(c"localhost:", "localhost", "6379")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_cvars_uses_a_connection_pool(
        _pyshinqlx_setup: (),
        #[case] address: &'static CStr,
        #[case] host: &str,
        #[case] port: &str,
    ) {
        let cvars = RedisCvars {
            address: Some(address),
            ..default_cvars()
        };
        with_redis(redis_engine(cvars), |py, redis| {
            let connection = redis.connect().expect("could not connect");

            let pool = py
                .get_type::<Redis>()
                .getattr("_pool")
                .expect("no pool created");
            assert_eq!(kwarg(&pool, "host"), host);
            assert_eq!(kwarg(&pool, "port"), port);
            assert_eq!(kwarg(&pool, "db"), "3");
            assert_eq!(kwarg(&pool, "password"), "secret");
            assert_eq!(kwarg(&pool, "decode_responses"), "True");
            assert_eq!(kwarg(&connection, "decode_responses"), "True");
            assert!(
                connection
                    .getattr("kwargs")
                    .and_then(|kwargs| kwargs.get_item("connection_pool"))
                    .is_ok_and(|used_pool| used_pool.is(&pool))
            );
            assert!(redis.connect().is_ok_and(|again| again.is(&connection)));
            assert!(redis.getattr("_conn").is_ok_and(|conn| conn.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_unix_socket_cvar_does_not_use_a_pool(_pyshinqlx_setup: ()) {
        let cvars = RedisCvars {
            address: Some(c"/tmp/redis.sock"),
            unix_socket: Some(c"1"),
            ..default_cvars()
        };
        with_redis(redis_engine(cvars), |py, redis| {
            let connection = redis.connect().expect("could not connect");

            assert_eq!(kwarg(&connection, "unix_socket_path"), "/tmp/redis.sock");
            assert_eq!(kwarg(&connection, "db"), "3");
            assert_eq!(kwarg(&connection, "password"), "secret");
            assert_eq!(kwarg(&connection, "decode_responses"), "True");
            assert_eq!(kwarg(&connection, "socket_timeout"), "None");
            assert_eq!(kwarg(&connection, "protocol"), "2");
            assert!(py.get_type::<Redis>().getattr("_pool").is_err());
        });
    }

    #[rstest]
    #[case::address(
        RedisCvars { address: None, ..default_cvars() },
        "cvar qlx_redisAddress misconfigured"
    )]
    #[case::database_missing(
        RedisCvars { database: None, ..default_cvars() },
        "cvar qlx_redisDatabase misconfigured."
    )]
    #[case::database_not_a_number(
        RedisCvars { database: Some(c"abc"), ..default_cvars() },
        "cvar qlx_redisDatabase misconfigured."
    )]
    #[case::unix_socket(
        RedisCvars { unix_socket: None, ..default_cvars() },
        "cvar qlx_redisUnixSocket misconfigured."
    )]
    #[case::password(
        RedisCvars { password: None, ..default_cvars() },
        "cvar qlx_redisPassword misconfigured."
    )]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_misconfigured_cvars_fails(
        _pyshinqlx_setup: (),
        #[case] cvars: RedisCvars,
        #[case] message: &str,
    ) {
        with_redis(redis_engine(cvars), |py, redis| {
            let result = redis.connect();

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                    && err.value(py).to_string() == message)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_without_main_engine_fails(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let result = redis.connect();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_parameters_builds_an_instance_pool(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = redis
                .connect_with_parameters("db.example:6400", 2, false, Some("pw"))
                .expect("could not connect");

            let pool = redis.getattr("_pool").expect("no pool created");
            assert_eq!(kwarg(&pool, "host"), "db.example");
            assert_eq!(kwarg(&pool, "port"), "6400");
            assert_eq!(kwarg(&pool, "db"), "2");
            assert_eq!(kwarg(&pool, "password"), "pw");
            assert_eq!(kwarg(&pool, "decode_responses"), "True");
            assert!(
                redis
                    .connect_with_parameters("other:1", 9, false, None)
                    .is_ok_and(|again| again.is(&connection))
            );
            assert!(py.get_type::<Redis>().getattr("_conn").is_err());
        });
    }

    #[rstest]
    #[case("db.example", "6379")]
    #[case("db.example:", "6379")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_parameters_defaults_the_port(
        _pyshinqlx_setup: (),
        #[case] host: &str,
        #[case] port: &str,
    ) {
        with_fake_redis(|_, redis| {
            redis
                .connect_with_parameters(host, 0, false, None)
                .expect("could not connect");

            let pool = redis.getattr("_pool").expect("no pool created");
            assert_eq!(kwarg(&pool, "port"), port);
            assert_eq!(kwarg(&pool, "password"), "None");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_parameters_and_unix_socket_does_not_use_a_pool(_pyshinqlx_setup: ()) {
        with_fake_redis(|_, redis| {
            let connection = redis
                .connect_with_parameters("/tmp/other.sock", 1, true, Some("pw"))
                .expect("could not connect");

            assert_eq!(kwarg(&connection, "unix_socket_path"), "/tmp/other.sock");
            assert_eq!(kwarg(&connection, "db"), "1");
            assert_eq!(kwarg(&connection, "password"), "pw");
            assert_eq!(kwarg(&connection, "socket_timeout"), "None");
            assert_eq!(kwarg(&connection, "protocol"), "2");
            assert!(redis.getattr("_pool").is_ok_and(|pool| pool.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connect_with_host_argument_connects_to_that_host(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let kwargs = PyDict::new(py);
            kwargs
                .set_item("database", 4)
                .expect("this should not happen");

            let connection = redis
                .call_method("connect", ("db.example:6400",), Some(&kwargs))
                .expect("could not connect");

            let pool = redis.getattr("_pool").expect("no pool created");
            assert_eq!(kwarg(&pool, "host"), "db.example");
            assert_eq!(kwarg(&pool, "db"), "4");
            assert_eq!(kwarg(&connection, "decode_responses"), "True");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn close_disconnects_the_instance_pool(_pyshinqlx_setup: ()) {
        with_fake_redis(|_, redis| {
            connected(&redis);
            let pool = redis.getattr("_pool").expect("no pool created");

            assert!(redis.close().is_ok());

            assert_eq!(kwarg_free(&pool, "disconnects"), "1");
            assert!(redis.getattr("_conn").is_ok_and(|conn| conn.is_none()));
            assert!(redis.getattr("_pool").is_ok_and(|pool| pool.is_none()));
        });
    }

    fn kwarg_free(object: &Bound<'_, PyAny>, name: &str) -> String {
        object
            .getattr(name)
            .map(|value| value.to_string())
            .expect("missing attribute")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn close_without_a_connection_succeeds(_pyshinqlx_setup: ()) {
        with_fake_redis(|_, redis| {
            assert!(redis.close().is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn close_of_the_last_instance_closes_the_default_connection(_pyshinqlx_setup: ()) {
        with_redis(redis_engine(default_cvars()), |py, redis| {
            redis.connect().expect("could not connect");
            let redis_type = py.get_type::<Redis>();
            let pool = redis_type.getattr("_pool").expect("no pool created");

            assert!(redis.close().is_ok());

            assert_eq!(kwarg_free(&pool, "disconnects"), "1");
            assert!(redis_type.getattr("_conn").is_ok_and(|conn| conn.is_none()));
            assert!(redis_type.getattr("_pool").is_ok_and(|pool| pool.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn close_keeps_the_default_connection_while_other_instances_use_it(_pyshinqlx_setup: ()) {
        with_redis(redis_engine(default_cvars()), |py, redis| {
            let plugin = test_plugin(py)
                .call0()
                .expect("could not create the plugin");
            let _other = Bound::new(py, Redis::py_new(py, plugin.as_any()))
                .expect("could not create the second database");
            let connection = redis.connect().expect("could not connect");

            assert!(redis.close().is_ok());

            assert!(
                py.get_type::<Redis>()
                    .getattr("_conn")
                    .is_ok_and(|conn| conn.is(&connection))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_and_deleting_databases_counts_the_instances(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let redis_type = py.get_type::<Redis>();
            assert_eq!(kwarg_free(&redis_type, "_counter"), "1");

            let plugin = test_plugin(py)
                .call0()
                .expect("could not create the plugin");
            let other = Bound::new(py, Redis::py_new(py, plugin.as_any()))
                .expect("could not create the second database");
            assert_eq!(kwarg_free(&redis_type, "_counter"), "2");

            other.call_method0("__del__").expect("could not delete");
            assert_eq!(kwarg_free(&redis_type, "_counter"), "1");

            redis.call_method0("__del__").expect("could not delete");
            other.call_method0("__del__").expect("could not delete");
            assert_eq!(kwarg_free(&redis_type, "_counter"), "0");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn item_access_reads_writes_and_deletes_keys(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);

            assert!(redis.contains("key").is_ok_and(|contained| !contained));
            redis.set_item("key", "value").expect("could not set item");
            assert!(redis.contains("key").is_ok_and(|contained| contained));
            assert_eq!(stored(&connection, "key"), Some("value".to_string()));
            assert!(
                redis
                    .get_item("key")
                    .is_ok_and(|value| value.to_string() == "value")
            );

            redis.del_item("key").expect("could not delete item");
            assert!(redis.contains("key").is_ok_and(|contained| !contained));

            let missing_get = redis.get_item("key");
            assert!(
                missing_get.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)
                    && err.value(py).to_string()
                        == "\"The key 'key' is not present in the database.\"")
            );
            let missing_delete = redis.del_item("key");
            assert!(missing_delete.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn item_assignment_that_redis_rejects_fails(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            connected(&redis);

            let result = redis.set_item("fail", "value");

            assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unknown_attributes_are_looked_up_on_the_connection(_pyshinqlx_setup: ()) {
        with_fake_redis(|_, redis| {
            let connection = connected(&redis);

            assert!(redis.getattr("_pool").is_ok_and(|pool| !pool.is_none()));
            assert!(redis.getattr("r").is_ok_and(|r| r.is(&connection)));
            assert!(
                redis
                    .getattr("data")
                    .is_ok_and(|data| data.len().is_ok_and(|n| n == 0))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_permission_stores_the_level_per_steam_id(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"76561198000000001"), |py, redis| {
            let connection = connected(&redis);
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            redis
                .set_permission(player.as_any(), 3)
                .expect("could not set permission");
            redis
                .set_permission(PyInt::new(py, 42).as_any(), 1)
                .expect("could not set permission");

            assert_eq!(
                stored(&connection, "minqlx:players:1234567890:permission"),
                Some("3".to_string())
            );
            assert_eq!(
                stored(&connection, "minqlx:players:42:permission"),
                Some("1".to_string())
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_permission_returns_zero_for_unknown_players(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"76561198000000001"), |py, redis| {
            connected(&redis);
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            assert!(
                redis
                    .get_permission(player.as_any())
                    .is_ok_and(|level| level == 0)
            );
            assert!(
                redis
                    .get_permission(PyInt::new(py, 42).as_any())
                    .is_ok_and(|level| level == 0)
            );
            assert!(
                redis
                    .get_permission(PyString::new(py, "42").as_any())
                    .is_ok_and(|level| level == 0)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_permission_returns_stored_levels(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"76561198000000001"), |py, redis| {
            let connection = connected(&redis);
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            redis
                .set_permission(player.as_any(), 4)
                .expect("could not set permission");
            redis
                .set_permission(PyInt::new(py, 42).as_any(), 2)
                .expect("could not set permission");

            assert!(
                redis
                    .get_permission(player.as_any())
                    .is_ok_and(|level| level == 4)
            );
            assert!(
                redis
                    .get_permission(PyInt::new(py, 42).as_any())
                    .is_ok_and(|level| level == 2)
            );
            assert!(
                redis
                    .get_permission(PyString::new(py, "42").as_any())
                    .is_ok_and(|level| level == 2)
            );
            assert!(
                redis
                    .has_permission(player.as_any(), 4)
                    .is_ok_and(|has| has)
            );
            assert!(
                redis
                    .has_permission(player.as_any(), 5)
                    .is_ok_and(|has| !has)
            );
            drop(connection);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_permission_of_the_owner_is_five(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"1234567890"), |py, redis| {
            connected(&redis);
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            assert!(
                redis
                    .get_permission(player.as_any())
                    .is_ok_and(|level| level == 5)
            );
            assert!(
                redis
                    .get_permission(PyInt::new(py, 1234567890).as_any())
                    .is_ok_and(|level| level == 5)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_permission_with_an_invalid_player_fails(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"76561198000000001"), |py, redis| {
            connected(&redis);

            let result = redis.get_permission(PyString::new(py, "abc").as_any());

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                    && err.value(py).to_string()
                        == "Invalid player. Use either a shinqlx.Player instance or a SteamID64.")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_permission_with_a_corrupt_stored_level_fails(_pyshinqlx_setup: ()) {
        with_redis(owner_engine(c"76561198000000001"), |py, redis| {
            let connection = connected(&redis);
            connection
                .getattr("data")
                .and_then(|data| data.set_item("minqlx:players:42:permission", "x"))
                .expect("could not store level");

            let result = redis.get_permission(PyInt::new(py, 42).as_any());

            assert!(
                result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                    && err.value(py).to_string() == "invalid literal for int() with base 10: 'x'")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flags_are_stored_cleared_and_read(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            let steam_id = PyInt::new(py, 42);

            assert!(
                redis
                    .get_flag(player.as_any(), "admin", true)
                    .is_ok_and(|flag| flag)
            );
            assert!(
                redis
                    .get_flag(player.as_any(), "admin", false)
                    .is_ok_and(|flag| !flag)
            );

            redis
                .set_flag(player.as_any(), "admin", true)
                .expect("could not set flag");
            redis
                .set_flag(steam_id.as_any(), "muted", true)
                .expect("could not set flag");
            assert_eq!(
                stored(&connection, "minqlx:players:1234567890:flags:admin"),
                Some("1".to_string())
            );
            assert_eq!(
                stored(&connection, "minqlx:players:42:flags:muted"),
                Some("1".to_string())
            );
            assert!(
                redis
                    .get_flag(player.as_any(), "admin", false)
                    .is_ok_and(|flag| flag)
            );

            redis
                .clear_flag(player.as_any(), "admin")
                .expect("could not clear flag");
            assert_eq!(
                stored(&connection, "minqlx:players:1234567890:flags:admin"),
                Some("0".to_string())
            );
            assert!(
                redis
                    .get_flag(player.as_any(), "admin", true)
                    .is_ok_and(|flag| !flag)
            );
            assert!(
                redis
                    .get_flag(steam_id.as_any(), "muted", false)
                    .is_ok_and(|flag| flag)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mset_accepts_a_dict_and_keyword_arguments(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let dict = PyDict::new(py);
            dict.set_item("a", 1).expect("this should not happen");
            let kwargs = PyDict::new(py);
            kwargs.set_item("b", 2).expect("this should not happen");

            redis
                .mset(
                    &PyTuple::new(py, [dict]).expect("this should not happen"),
                    Some(&kwargs),
                )
                .expect("could not mset");

            assert_eq!(stored(&connection, "a"), Some("1".to_string()));
            assert_eq!(stored(&connection, "b"), Some("2".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mset_and_msetnx_reject_other_positional_arguments(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            connected(&redis);
            let two_args = PyTuple::new(py, [1, 2]).expect("this should not happen");
            let not_a_dict = PyTuple::new(py, [1]).expect("this should not happen");

            for (result, message) in [
                (
                    redis.mset(&two_args, None),
                    "MSET requires **kwargs or a single dict arg",
                ),
                (
                    redis.mset(&not_a_dict, None),
                    "MSET requires **kwargs or a single dict arg",
                ),
                (
                    redis.msetnx(&two_args, None),
                    "MSETNX requires **kwargs or a single dict arg",
                ),
                (
                    redis.msetnx(&not_a_dict, None),
                    "MSETNX requires **kwargs or a single dict arg",
                ),
            ] {
                let err = result.expect_err("should have been rejected");
                assert_eq!(error_details(py, &err), redis_error_details(message));
            }
        });
    }

    fn redis_error_details(message: &str) -> (String, String) {
        ("RedisError".to_string(), message.to_string())
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msetnx_only_sets_keys_that_do_not_exist(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let kwargs = PyDict::new(py);
            kwargs.set_item("a", 1).expect("this should not happen");

            let first = redis
                .msetnx(&PyTuple::empty(py), Some(&kwargs))
                .expect("could not msetnx");
            let second = redis
                .msetnx(&PyTuple::empty(py), Some(&kwargs))
                .expect("could not msetnx");

            assert_eq!(first.to_string(), "True");
            assert_eq!(second.to_string(), "False");
            assert_eq!(stored(&connection, "a"), Some("1".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn zadd_passes_a_dict_and_keyword_arguments_through(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let scores = PyDict::new(py);
            scores.set_item("a", 1).expect("this should not happen");
            let kwargs = PyDict::new(py);
            kwargs.set_item("nx", true).expect("this should not happen");

            redis
                .zadd(
                    "z",
                    &PyTuple::new(py, [scores]).expect("this should not happen"),
                    Some(&kwargs),
                )
                .expect("could not zadd");

            assert_eq!(
                calls(&connection),
                "[('zadd', 'z', {'a': 1}, {'nx': True})]"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn zadd_converts_score_and_value_pairs_to_a_mapping(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let pairs = PyTuple::new(py, [10, 1, 20, 2]).expect("this should not happen");

            redis.zadd("z", &pairs, None).expect("could not zadd");

            assert_eq!(
                calls(&connection),
                "[('zadd', 'z', {'1': '10', '2': '20'}, {})]"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn zadd_rejects_an_odd_number_of_arguments(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            connected(&redis);
            let odd = PyTuple::new(py, [1, 2, 3]).expect("this should not happen");

            let err = redis
                .zadd("z", &odd, None)
                .expect_err("should have been rejected");

            assert_eq!(
                error_details(py, &err),
                redis_error_details("ZADD requires an equal number of values and scores")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn zincrby_passes_value_and_amount_by_name(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let explicit = PyDict::new(py);
            explicit
                .set_item("value", "m")
                .expect("this should not happen");
            explicit
                .set_item("amount", 2)
                .expect("this should not happen");
            let defaulted = PyDict::new(py);
            defaulted
                .set_item("value", "n")
                .expect("this should not happen");

            redis
                .call_method("zincrby", ("z",), Some(&explicit))
                .expect("could not zincrby");
            redis
                .call_method("zincrby", ("z",), Some(&defaulted))
                .expect("could not zincrby");

            assert_eq!(
                calls(&connection),
                "[('zincrby', 'z', 2, 'm'), ('zincrby', 'z', 1, 'n')]"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setex_passes_value_and_time_by_name(_pyshinqlx_setup: ()) {
        with_fake_redis(|py, redis| {
            let connection = connected(&redis);
            let kwargs = PyDict::new(py);
            kwargs
                .set_item("value", "v")
                .expect("this should not happen");
            kwargs.set_item("time", 60).expect("this should not happen");

            redis
                .call_method("setex", ("k",), Some(&kwargs))
                .expect("could not setex");

            assert_eq!(calls(&connection), "[('setex', 'k', 60, 'v')]");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lrem_accepts_value_and_an_optional_count(_pyshinqlx_setup: ()) {
        with_fake_redis(|_, redis| {
            let connection = connected(&redis);

            redis
                .call_method1("lrem", ("l", "v"))
                .expect("could not lrem");
            redis
                .call_method1("lrem", ("l", "w", 2))
                .expect("could not lrem");

            assert_eq!(
                calls(&connection),
                "[('lrem', 'l', 0, 'v'), ('lrem', 'l', 2, 'w')]"
            );
        });
    }

    #[rstest]
    #[case("zincrby", "amount", Some("1"))]
    #[case("lrem", "count", Some("0"))]
    #[case("lrem", "value", None)]
    #[case("setex", "time", None)]
    #[case("connect", "database", Some("0"))]
    #[case("get_flag", "default", Some("False"))]
    #[case("has_permission", "level", Some("5"))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn python_signatures_expose_the_defaults(
        _pyshinqlx_setup: (),
        #[case] method: &str,
        #[case] parameter: &str,
        #[case] default: Option<&str>,
    ) {
        Python::attach(|py| {
            let signature = py
                .import("inspect")
                .and_then(|inspect| {
                    inspect.call_method1("signature", (py.get_type::<Redis>().getattr(method)?,))
                })
                .unwrap_or_else(|err| panic!("no signature for {method}: {err}"));

            let parameter = signature
                .getattr("parameters")
                .and_then(|parameters| parameters.get_item(parameter))
                .expect("parameter missing in the signature");
            let default_value = parameter
                .getattr("default")
                .expect("parameter without default attribute");
            let empty = py
                .import("inspect")
                .and_then(|inspect| inspect.getattr("Parameter"))
                .and_then(|parameter_class| parameter_class.getattr("empty"))
                .expect("could not get the empty marker");

            match default {
                Some(expected) => assert_eq!(default_value.to_string(), expected),
                None => assert!(default_value.is(&empty)),
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setex_replaces_the_misnamed_setx(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let redis_type = py.get_type::<Redis>();

            assert!(redis_type.hasattr("setex").is_ok_and(|has| has));
            assert!(redis_type.hasattr("setx").is_ok_and(|has| !has));
        });
    }
}
