use super::prelude::*;
use super::{owner, pyshinqlx_get_logger};

use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::prelude::*;
use pyo3::{exceptions::PyRuntimeError, types::IntoPyDict};
use pyo3::{
    exceptions::{PyEnvironmentError, PyKeyError, PyNotImplementedError, PyValueError},
    intern,
    types::{PyBool, PyDict, PyString, PyTuple},
    PyTraverseError, PyVisit,
};

use core::cmp::max;

use itertools::Itertools;

#[pyclass(name = "AbstractDatabase", module = "database", subclass)]
pub(crate) struct AbstractDatabase {
    plugin: PyObject,
}

#[pymethods]
impl AbstractDatabase {
    #[new]
    fn py_new(_py: Python<'_>, plugin: Bound<'_, PyAny>) -> Self {
        Self {
            plugin: plugin.unbind(),
        }
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.plugin)?;
        Ok(())
    }

    fn __clear__(&mut self) {}

    #[getter(logger)]
    fn get_logger<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let plugin_name = self
            .plugin
            .bind(py)
            .get_type()
            .name()
            .map(|value| value.to_string())?;
        pyshinqlx_get_logger(py, Some(PyString::new(py, &plugin_name).into_any()))
    }

    /// Abstract method. Should set the permission of a player.
    #[allow(unused_variables)]
    fn set_permission(&self, player: Bound<'_, PyAny>, level: i32) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return the permission of a player.
    #[allow(unused_variables)]
    fn get_permission(&self, player: Bound<'_, PyAny>) -> PyResult<i32> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return whether or not a player has more than or equal
    /// to a certain permission level. Should only take a value of 0 to 5, where 0 is
    /// always True.
    #[allow(unused_variables)]
    #[pyo3(signature = (player, level=5), text_signature = "(player, level=5)")]
    fn has_permission(&self, player: Bound<'_, PyAny>, level: i32) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should set specified player flag to value.
    #[allow(unused_variables)]
    #[pyo3(signature = (player, flag, value=true), text_signature = "(player, flag, value=true)")]
    fn set_flag(&self, player: Bound<'_, PyAny>, flag: &str, value: bool) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Should clear specified player flag.
    fn clear_flag(&self, player: Bound<'_, PyAny>, flag: &str) -> PyResult<bool> {
        self.set_flag(player, flag, false)
    }

    /// Abstract method. Should return specified player flag
    #[allow(unused_variables)]
    #[pyo3(signature = (player, flag, default=false), text_signature = "(player, flag, default=false)")]
    fn get_flag(&self, player: Bound<'_, PyAny>, flag: &str, default: bool) -> PyResult<bool> {
        Err(PyNotImplementedError::new_err(
            "The abstract base database can't do database actions.",
        ))
    }

    /// Abstract method. Should return a connection to the database. Exactly what a
    /// "connection" obviously depends on the database, so the specifics will be up
    /// to the implementation.
    ///
    /// A :class:`shinqlx.Plugin` subclass can set
    fn connect<'py>(&self) -> PyResult<Bound<'py, PyAny>> {
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

/// A subclass of :class:`shinqlx.AbstractDatabase` providing support for Redis.
#[pyclass(name = "Redis", module = "database", extends = AbstractDatabase, dict)]
pub(crate) struct Redis {}

#[pymethods]
impl Redis {
    #[new]
    fn py_new(py: Python<'_>, plugin: Bound<'_, PyAny>) -> (Self, AbstractDatabase) {
        let redis_type = py.get_type::<Self>();
        let counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        let _ = redis_type.setattr(intern!(py, "_counter"), counter + 1);

        (
            Self {},
            AbstractDatabase {
                plugin: plugin.unbind(),
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
    fn get_redis<'py>(slf_: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        Self::connect(slf_, None, 0, false, None)
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

    fn __setitem__(slf_: &Bound<'_, Self>, key: &str, item: Bound<'_, PyAny>) -> PyResult<()> {
        let redis_connection = Self::get_redis(slf_)?;
        let returned = redis_connection
            .call_method1(intern!(slf_.py(), "set"), (key, item))
            .and_then(|value| value.extract::<bool>())?;

        if !returned {
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
        slf_: &Bound<'_, Self>,
        player: Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>() {
            format!("minqlx:players:{}:permission", rust_player.steam_id)
        } else {
            format!("minqlx:players:{}:permission", player.str()?)
        };

        Self::__setitem__(slf_, &key, level.into_pyobject(slf_.py())?.into_any())
    }

    /// Gets the permission of a player.
    fn get_permission(slf_: &Bound<'_, Self>, player: Bound<'_, PyAny>) -> PyResult<i32> {
        let steam_id = if let Ok(rust_player) = player.extract::<Player>() {
            Ok(rust_player.steam_id)
        } else if let Ok(rust_int) = player.extract::<i64>() {
            Ok(rust_int)
        } else if let Ok(rust_str) = player.extract::<String>() {
            rust_str.parse::<i64>().map_err(|_| {
                let error_msg = format!("invalid literal for int() with base 10: '{}'", rust_str);
                PyValueError::new_err(error_msg)
            })
        } else {
            Err(PyValueError::new_err(
                "Invalid player. Use either a shinqlx.Player instance or a SteamID64.",
            ))
        }?;

        if Some(steam_id) == owner()? {
            return Ok(5);
        }

        let key = format!("minqlx:players:{steam_id}:permission");
        if !Self::__contains__(slf_, &key)? {
            return Ok(0);
        }
        Self::__getitem__(slf_, &key).and_then(|value| {
            value.to_string().parse::<i32>().map_err(|_| {
                let error_msg = format!("invalid literal for int() with base 10: '{value}",);
                PyValueError::new_err(error_msg)
            })
        })
    }

    /// Checks if the player has higher than or equal to *level*.
    #[pyo3(name = "has_permission", signature = (player, level = 5), text_signature = "(player, level=5)")]
    fn has_permission(
        slf_: &Bound<'_, Self>,
        player: Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<bool> {
        Self::get_permission(slf_, player).map(|value| value >= level)
    }

    /// Sets specified player flag
    #[pyo3(name = "set_flag", signature = (player, flag, value = true), text_signature = "(player, flag, value = True)")]
    fn set_flag(
        slf_: &Bound<'_, Self>,
        player: Bound<'_, PyAny>,
        flag: &str,
        value: bool,
    ) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>() {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.str()?, flag)
        };

        let redis_value = if value { 1i32 } else { 0i32 };

        Self::__setitem__(slf_, &key, redis_value.into_pyobject(slf_.py())?.into_any())
    }

    /// returns the specified player flag
    #[pyo3(name = "get_flag", signature = (player, flag, default = false), text_signature = "(player, flag, default=False)")]
    fn get_flag(
        slf_: &Bound<'_, Self>,
        player: Bound<'_, PyAny>,
        flag: &str,
        default: bool,
    ) -> PyResult<bool> {
        let key = if let Ok(rust_player) = player.extract::<Player>() {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.str()?, flag)
        };

        if !Self::__contains__(slf_, &key)? {
            return Ok(default);
        }

        Self::__getitem__(slf_, &key).map(|value| value.to_string() != "0")
    }

    /// Returns a connection to a Redis database. If *host* is None, it will
    /// fall back to the settings in the config and ignore the rest of the arguments.
    /// It will also share the connection across any plugins using the default
    /// configuration. Passing *host* will make it connect to a specific database
    /// that is not shared at all. Subsequent calls to this will return the connection
    /// initialized the first call unless it has been closed.
    #[pyo3(name = "connect", signature = (host = None, database = 0, unix_socket = false, password = None), text_signature = "(host = None, database = 0, unix_socket = false, password = None)")]
    fn connect<'py>(
        slf_: &Bound<'py, Self>,
        host: Option<&str>,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Ok(redis_connection) = slf_.getattr(intern!(slf_.py(), "_conn")) {
            if !redis_connection.is_none() {
                return Ok(redis_connection);
            }
        }

        if let Ok(class_connection) = slf_
            .py()
            .get_type::<Redis>()
            .getattr(intern!(slf_.py(), "_conn"))
        {
            if !class_connection.is_none() {
                return Ok(class_connection);
            }
        }

        let py_redis = slf_.py().import(intern!(slf_.py(), "redis"))?;
        let strict_redis = py_redis.getattr(intern!(slf_.py(), "StrictRedis"))?;
        match host {
            None => {
                let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to main engine.",
                    ));
                };

                let Some(cvar_host) = main_engine.find_cvar("qlx_redisAddress") else {
                    return Err(PyValueError::new_err("cvar qlx_redisAddress misconfigured"));
                };
                let Some(redis_db_cvar) = main_engine
                    .find_cvar("qlx_redisDatabase")
                    .and_then(|cvar| cvar.get_string().parse::<i64>().ok())
                else {
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
                    return Err(PyValueError::new_err(
                        "cvar qlx_redisUnixSocket misconfigured.",
                    ));
                };
                let Some(password_cvar) = main_engine.find_cvar("qlx_redisPassword") else {
                    return Err(PyValueError::new_err(
                        "cvar qlx_redisPassword misconfigured.",
                    ));
                };

                let class_connection = if unix_socket_cvar {
                    strict_redis.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                (
                                    "unix_socket_path",
                                    PyString::new(slf_.py(), &cvar_host.get_string()).into_any(),
                                ),
                                ("db", redis_db_cvar.into_pyobject(slf_.py())?.into_any()),
                                (
                                    "password",
                                    PyString::new(slf_.py(), &password_cvar.get_string())
                                        .into_any(),
                                ),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )
                } else {
                    let hostname = cvar_host.get_string();
                    let (redis_hostname, port) = hostname
                        .split_once(':')
                        .unwrap_or((hostname.as_ref(), "6379"));
                    let redis_port = if port.is_empty() { "6379" } else { port };
                    let connection_pool = py_redis.getattr(intern!(slf_.py(), "ConnectionPool"))?;

                    let redis_pool = connection_pool.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                ("host", PyString::new(slf_.py(), redis_hostname).into_any()),
                                ("port", PyString::new(slf_.py(), redis_port).into_any()),
                                ("db", redis_db_cvar.into_pyobject(slf_.py())?.into_any()),
                                (
                                    "password",
                                    PyString::new(slf_.py(), &password_cvar.get_string())
                                        .into_any(),
                                ),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )?;
                    slf_.py()
                        .get_type::<Redis>()
                        .setattr(intern!(slf_.py(), "_pool"), &redis_pool)?;
                    strict_redis.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                ("connection_pool", redis_pool),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )
                }?;
                slf_.py()
                    .get_type::<Redis>()
                    .setattr(intern!(slf_.py(), "_conn"), &class_connection)?;
                slf_.setattr(intern!(slf_.py(), "_conn"), slf_.py().None())?;
                Ok(class_connection)
            }
            Some(hostname) => {
                let instance_connection = if unix_socket {
                    strict_redis.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                (
                                    "unix_socket_path",
                                    PyString::new(slf_.py(), hostname).into_any(),
                                ),
                                ("db", database.into_pyobject(slf_.py())?.into_any()),
                                ("password", password.into_pyobject(slf_.py())?.into_any()),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )
                } else {
                    let (redis_hostname, port) =
                        hostname.split_once(':').unwrap_or((hostname, "6379"));
                    let redis_port = if port.is_empty() { "6379" } else { port };
                    let connection_pool = py_redis.getattr(intern!(slf_.py(), "ConnectionPool"))?;

                    let redis_pool = connection_pool.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                ("host", PyString::new(slf_.py(), redis_hostname).into_any()),
                                ("port", PyString::new(slf_.py(), redis_port).into_any()),
                                ("db", database.into_pyobject(slf_.py())?.into_any()),
                                ("password", password.into_pyobject(slf_.py())?.into_any()),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )?;
                    slf_.setattr(intern!(slf_.py(), "_pool"), &redis_pool)?;
                    strict_redis.call(
                        PyTuple::empty(slf_.py()),
                        Some(
                            &[
                                ("connection_pool", redis_pool),
                                (
                                    "decode_responses",
                                    PyBool::new(slf_.py(), true).to_owned().into_any(),
                                ),
                            ]
                            .into_py_dict(slf_.py())?,
                        ),
                    )
                }?;
                slf_.setattr(intern!(slf_.py(), "_conn"), &instance_connection)?;
                Ok(instance_connection)
            }
        }
    }

    /// Close the Redis connection if the config was overridden. Otherwise only do so
    /// if this is the last plugin using the default connection.
    fn close(slf_: &Bound<'_, Self>) -> PyResult<()> {
        if slf_
            .getattr(intern!(slf_.py(), "_conn"))
            .is_ok_and(|instance_connection| !instance_connection.is_none())
        {
            slf_.setattr(intern!(slf_.py(), "_conn"), slf_.py().None())?;
            if let Ok(instance_pool) = slf_.getattr(intern!(slf_.py(), "_pool")) {
                if !instance_pool.is_none() {
                    instance_pool.call_method0(intern!(slf_.py(), "disconnect"))?;
                    slf_.setattr(intern!(slf_.py(), "_pool"), slf_.py().None())?;
                }
            }
        };

        let redis_type = slf_.py().get_type::<Redis>();
        let class_counter = redis_type
            .getattr(intern!(slf_.py(), "_counter"))
            .and_then(|value| value.extract::<i32>())
            .unwrap_or(0);
        if class_counter <= 1
            && redis_type
                .getattr(intern!(slf_.py(), "_conn"))
                .is_ok_and(|class_connection| !class_connection.is_none())
        {
            redis_type.setattr(intern!(slf_.py(), "_conn"), slf_.py().None())?;
            if let Ok(class_pool) = redis_type.getattr(intern!(slf_.py(), "_pool")) {
                if !class_pool.is_none() {
                    class_pool.call_method0(intern!(slf_.py(), "disconnect"))?;
                    redis_type.setattr(intern!(slf_.py(), "_pool"), slf_.py().None())?;
                }
            }
        }
        Ok(())
    }

    #[pyo3(name = "mset", signature = (*args, **kwargs))]
    fn mset<'py>(
        slf_: &Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_module = slf_.py().import(intern!(slf_.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(slf_.py(), "RedisError"))?;

        let mapping = PyDict::new(slf_.py());

        if args.len() > 1 {
            let error = redis_error.call1((intern!(
                slf_.py(),
                "MSET requires **kwargs or a single dict arg"
            ),))?;
            return Err(PyErr::from_value(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0) else {
                let error = redis_error.call1((intern!(
                    slf_.py(),
                    "MSET requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.downcast::<PyDict>()?.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = Self::get_redis(slf_)?;
        redis_connection.call_method1(intern!(slf_.py(), "mset"), (mapping,))
    }

    #[pyo3(name = "msetnx", signature = (*args, **kwargs))]
    fn msetnx<'py>(
        slf_: &Bound<'py, Self>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_module = slf_.py().import(intern!(slf_.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(slf_.py(), "RedisError"))?;

        let mapping = PyDict::new(slf_.py());

        if args.len() > 1 {
            let error = redis_error.call1((intern!(
                slf_.py(),
                "MSENXT requires **kwargs or a single dict arg"
            ),))?;
            return Err(PyErr::from_value(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0) else {
                let error = redis_error.call1((intern!(
                    slf_.py(),
                    "MSETNX requires **kwargs or a single dict arg"
                ),))?;
                return Err(PyErr::from_value(error));
            };
            mapping.update(dict_arg.downcast::<PyDict>()?.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = Self::get_redis(slf_)?;
        redis_connection.call_method1(intern!(slf_.py(), "msetnx"), (mapping,))
    }

    #[pyo3(name = "zadd", signature = (name, *args, **kwargs))]
    fn zadd<'py>(
        slf_: &Bound<'py, Self>,
        name: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = Self::get_redis(slf_)?;

        if args.len() == 1 && args.get_item(0)?.is_instance_of::<PyDict>() {
            let args_tuple = PyTuple::new(slf_.py(), [name].iter())?
                .as_sequence()
                .concat(args.as_sequence())?
                .to_tuple()?;
            return redis_connection.call_method(intern!(slf_.py(), "zadd"), args_tuple, kwargs);
        }

        let redis_module = slf_.py().import(intern!(slf_.py(), "redis"))?;
        let redis_error = redis_module.getattr(intern!(slf_.py(), "RedisError"))?;

        if args.len() % 2 != 0 {
            let error = redis_error.call1((intern!(
                slf_.py(),
                "ZADD requires an equal number of values and scores"
            ),))?;
            return Err(PyErr::from_value(error));
        }
        let pieces: Vec<(String, String)> = args
            .iter()
            .map(|item| item.to_string())
            .tuples()
            .map(|(a, b)| (b, a))
            .collect();

        redis_connection.call_method(
            intern!(slf_.py(), "zadd"),
            (name, pieces.into_py_dict(slf_.py())?),
            kwargs,
        )
    }

    #[pyo3(name = "zincrby", signature = (name, *, value, amount), text_signature = "(name, *, value, amount)")]
    fn zincrby<'py>(
        slf_: &Bound<'py, Self>,
        name: &str,
        value: Bound<'py, PyAny>,
        amount: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = Self::get_redis(slf_)?;

        redis_connection.call_method1(intern!(slf_.py(), "zincrby"), (name, amount, value))
    }

    #[pyo3(name = "setx", signature = (name, *, value, time), text_signature = "(name, *, value, time)")]
    fn setx<'py>(
        slf_: &Bound<'py, Self>,
        name: &str,
        value: Bound<'py, PyAny>,
        time: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = Self::get_redis(slf_)?;

        redis_connection.call_method1(intern!(slf_.py(), "setx"), (name, value, time))
    }

    #[pyo3(name = "lrem", signature = (name, *, value, count), text_signature = "(name, *, value, count)")]
    fn lrem<'py>(
        slf_: &Bound<'py, Self>,
        name: &str,
        value: Bound<'py, PyAny>,
        count: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let redis_connection = Self::get_redis(slf_)?;

        redis_connection.call_method1(intern!(slf_.py(), "lrem"), (name, value, count))
    }
}
