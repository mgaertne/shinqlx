use super::prelude::*;
use super::{owner, pyshinqlx_get_logger};

use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::prelude::*;
#[cfg(not(feature = "rust-redis"))]
use pyo3::types::{IntoPyDict, PyDelta, PyFloat, PyInt};
#[cfg(feature = "rust-redis")]
use pyo3::{
    exceptions::PyConnectionError,
    types::{PyMapping, PySet},
};
use pyo3::{
    exceptions::{PyEnvironmentError, PyKeyError, PyNotImplementedError, PyValueError},
    intern,
    types::{PyDict, PyTuple},
    PyTraverseError, PyVisit,
};

#[cfg(feature = "rust-redis")]
use redis::{Cmd, Commands, ExistenceCheck, Pipeline, RedisResult, SetExpiry, SetOptions};

#[cfg(feature = "rust-redis")]
use alloc::vec::IntoIter;
#[cfg(feature = "rust-redis")]
use arc_swap::ArcSwapOption;
use core::cmp::max;
#[cfg(feature = "rust-redis")]
use core::num::NonZeroUsize;
use itertools::Itertools;

#[pyclass(name = "AbstractDatabase", module = "database", subclass)]
pub(crate) struct AbstractDatabase {
    plugin: PyObject,
}

#[pymethods]
impl AbstractDatabase {
    #[new]
    fn py_new(_py: Python<'_>, plugin: PyObject) -> Self {
        Self { plugin }
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

/// A subclass of :class:`shinqlx.AbstractDatabase` providing support for Redis.
#[pyclass(name = "Redis", module = "database", extends = AbstractDatabase, dict)]
#[cfg(not(feature = "rust-redis"))]
pub(crate) struct Redis {}

#[cfg(not(feature = "rust-redis"))]
#[pymethods]
impl Redis {
    #[new]
    fn py_new(py: Python<'_>, plugin: PyObject) -> (Self, AbstractDatabase) {
        let redis_type = py.get_type_bound::<Self>();
        let counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        let _ = redis_type.setattr(intern!(py, "_counter"), counter + 1);

        (Self {}, AbstractDatabase { plugin })
    }

    fn __del__(slf_: &Bound<'_, Self>, py: Python<'_>) -> PyResult<()> {
        Self::close(slf_, py)?;
        let redis_type = py.get_type_bound::<Redis>();
        let counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|py_counter| py_counter.extract::<i32>())
            .unwrap_or(0);
        redis_type.setattr(intern!(py, "_counter"), max(0, counter - 1))?;

        Ok(())
    }

    #[getter(r)]
    fn get_redis(slf_: &Bound<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        Self::connect(slf_, py, None, 0, false, None)
    }

    fn __contains__(slf_: &Bound<'_, Self>, py: Python<'_>, key: &str) -> PyResult<bool> {
        let redis_connection = Self::get_redis(slf_, py)?;
        redis_connection
            .call_method1(py, intern!(py, "exists"), (key,))
            .map(|value| value.to_string() != "0")
    }

    fn __getitem__(slf_: &Bound<'_, Self>, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        let redis_connection = Self::get_redis(slf_, py)?;
        redis_connection
            .call_method1(py, intern!(py, "get"), (key,))
            .map_err(|_| {
                let error_msg = format!("The key '{key}' is not present in the database.");
                PyKeyError::new_err(error_msg)
            })
    }

    fn __setitem__(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        key: &str,
        item: PyObject,
    ) -> PyResult<()> {
        let redis_connection = Self::get_redis(slf_, py)?;
        let returned = redis_connection
            .call_method1(py, intern!(py, "set"), (key, item))
            .and_then(|value| value.extract::<bool>(py))?;

        if !returned {
            let error_msg = format!("The key '{key}' is not present in the database.");
            return Err(PyKeyError::new_err(error_msg));
        }

        Ok(())
    }

    fn __delitem__(slf_: &Bound<'_, Self>, py: Python<'_>, key: &str) -> PyResult<()> {
        let redis_connection = Self::get_redis(slf_, py)?;
        let returned = redis_connection
            .call_method1(py, intern!(py, "delete"), (key,))
            .and_then(|value| value.extract::<bool>(py))?;

        if !returned {
            let error_msg = format!("The key '{key}' is not present in the database.");
            return Err(PyKeyError::new_err(error_msg));
        }

        Ok(())
    }

    fn __getattr__(slf_: &Bound<'_, Self>, py: Python<'_>, attr: &str) -> PyResult<PyObject> {
        if ["_conn", "_pool"].contains(&attr) {
            return Ok(py.None());
        }
        let redis_connection = Self::get_redis(slf_, py)?;
        redis_connection.getattr(py, attr)
    }

    /// Sets the permission of a player.
    #[pyo3(name = "set_permission")]
    fn set_permission(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        level: i32,
    ) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:permission", rust_player.steam_id)
        } else {
            format!("minqlx:players:{}:permission", player.bind(py).str()?)
        };

        Self::__setitem__(slf_, py, &key, level.into_py(py))
    }

    /// Gets the permission of a player.
    fn get_permission(slf_: &Bound<'_, Self>, py: Python<'_>, player: PyObject) -> PyResult<i32> {
        let steam_id = if let Ok(rust_player) = player.extract::<Player>(py) {
            Ok(rust_player.steam_id)
        } else if let Ok(rust_int) = player.extract::<i64>(py) {
            Ok(rust_int)
        } else if let Ok(rust_str) = player.extract::<String>(py) {
            rust_str.parse::<i64>().map_err(|_| {
                let error_msg = format!("invalid literal for int() with base 10: '{}'", rust_str);
                PyValueError::new_err(error_msg)
            })
        } else {
            Err(PyValueError::new_err(
                "Invalid player. Use either a shinqlx.Player instance or a SteamID64.",
            ))
        }?;

        if Some(steam_id) == owner(py)? {
            return Ok(5);
        }

        let key = format!("minqlx:players:{steam_id}:permission");
        if !Self::__contains__(slf_, py, &key)? {
            return Ok(0);
        }
        Self::__getitem__(slf_, py, &key).and_then(|value| value.extract::<i32>(py))
    }

    /// Checks if the player has higher than or equal to *level*.
    #[pyo3(name = "has_permission", signature = (player, level = 5), text_signature = "(player, level=5)")]
    fn has_permission(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        level: i32,
    ) -> PyResult<bool> {
        Self::get_permission(slf_, py, player).map(|value| value >= level)
    }

    /// Sets specified player flag
    #[pyo3(name = "set_flag", signature = (player, flag, value = true), text_signature = "(player, flag, value = True)")]
    fn set_flag(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        flag: &str,
        value: bool,
    ) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.bind(py).str()?, flag)
        };

        let redis_value = if value { 1i32 } else { 0i32 };

        Self::__setitem__(slf_, py, &key, redis_value.into_py(py))
    }

    /// returns the specified player flag
    #[pyo3(name = "get_flag", signature = (player, flag, default = false), text_signature = "(player, flag, default=False)")]
    fn get_flag(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        player: PyObject,
        flag: &str,
        default: bool,
    ) -> PyResult<bool> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.bind(py).str()?, flag)
        };

        if !Self::__contains__(slf_, py, &key)? {
            return Ok(default);
        }

        Self::__getitem__(slf_, py, &key).map(|value| {
            value
                .extract::<i32>(py)
                .is_ok_and(|extracted| extracted != 0)
        })
    }

    /// Returns a connection to a Redis database. If *host* is None, it will
    /// fall back to the settings in the config and ignore the rest of the arguments.
    /// It will also share the connection across any plugins using the default
    /// configuration. Passing *host* will make it connect to a specific database
    /// that is not shared at all. Subsequent calls to this will return the connection
    /// initialized the first call unless it has been closed.
    #[pyo3(name = "connect", signature = (host = None, database = 0, unix_socket = false, password = None), text_signature = "(host = None, database = 0, unix_socket = false, password = None)")]
    fn connect(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        host: Option<&str>,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<PyObject> {
        if let Ok(redis_connection) = slf_.getattr(intern!(py, "_conn")) {
            if !redis_connection.is_none() {
                return Ok(redis_connection.unbind());
            }
        }

        if let Ok(class_connection) = py.get_type_bound::<Redis>().getattr(intern!(py, "_conn")) {
            if !class_connection.is_none() {
                return Ok(class_connection.unbind());
            }
        }

        let py_redis = py.import_bound(intern!(py, "redis"))?;
        let strict_redis = py_redis.getattr(intern!(py, "StrictRedis"))?;
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
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("unix_socket_path", cvar_host.get_string().into_py(py)),
                                ("db", redis_db_cvar.into_py(py)),
                                ("password", password_cvar.get_string().into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )
                } else {
                    let hostname = cvar_host.get_string();
                    let (redis_hostname, port) = hostname
                        .split_once(':')
                        .unwrap_or((hostname.as_ref(), "6379"));
                    let redis_port = if port.is_empty() { "6379" } else { port };
                    let connection_pool = py_redis.getattr(intern!(py, "ConnectionPool"))?;

                    let redis_pool = connection_pool.call(
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("host", redis_hostname.into_py(py)),
                                ("port", redis_port.into_py(py)),
                                ("db", redis_db_cvar.into_py(py)),
                                ("password", password_cvar.get_string().into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )?;
                    py.get_type_bound::<Redis>()
                        .setattr(intern!(py, "_pool"), &redis_pool)?;
                    strict_redis.call(
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("connection_pool", redis_pool.into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )
                }?;
                py.get_type_bound::<Redis>()
                    .setattr(intern!(py, "_conn"), &class_connection)?;
                slf_.setattr(intern!(py, "_conn"), py.None())?;
                Ok(class_connection.unbind())
            }
            Some(hostname) => {
                let instance_connection = if unix_socket {
                    strict_redis.call(
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("unix_socket_path", hostname.into_py(py)),
                                ("db", database.into_py(py)),
                                ("password", password.into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )
                } else {
                    let (redis_hostname, port) =
                        hostname.split_once(':').unwrap_or((hostname, "6379"));
                    let redis_port = if port.is_empty() { "6379" } else { port };
                    let connection_pool = py_redis.getattr(intern!(py, "ConnectionPool"))?;

                    let redis_pool = connection_pool.call(
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("host", redis_hostname.into_py(py)),
                                ("port", redis_port.into_py(py)),
                                ("db", database.into_py(py)),
                                ("password", password.into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )?;
                    slf_.setattr(intern!(py, "_pool"), &redis_pool)?;
                    strict_redis.call(
                        PyTuple::empty_bound(py),
                        Some(
                            &[
                                ("connection_pool", redis_pool.into_py(py)),
                                ("decode_responses", true.into_py(py)),
                            ]
                            .into_py_dict_bound(py),
                        ),
                    )
                }?;
                slf_.setattr(intern!(py, "_conn"), &instance_connection)?;
                Ok(instance_connection.unbind())
            }
        }
    }

    /// Close the Redis connection if the config was overridden. Otherwise only do so
    /// if this is the last plugin using the default connection.
    fn close(slf_: &Bound<'_, Self>, py: Python<'_>) -> PyResult<()> {
        if slf_
            .getattr(intern!(py, "_conn"))
            .is_ok_and(|instance_connection| !instance_connection.is_none())
        {
            slf_.setattr(intern!(py, "_conn"), py.None())?;
            if let Ok(instance_pool) = slf_.getattr(intern!(py, "_pool")) {
                if !instance_pool.is_none() {
                    instance_pool.call_method0(intern!(py, "disconnect"))?;
                    slf_.setattr(intern!(py, "_pool"), py.None())?;
                }
            }
        };

        let redis_type = py.get_type_bound::<Redis>();
        let class_counter = redis_type
            .getattr(intern!(py, "_counter"))
            .and_then(|value| value.extract::<i32>())
            .unwrap_or(0);
        if class_counter <= 1
            && redis_type
                .getattr(intern!(py, "_conn"))
                .is_ok_and(|class_connection| !class_connection.is_none())
        {
            redis_type.setattr(intern!(py, "_conn"), py.None())?;
            if let Ok(class_pool) = redis_type.getattr(intern!(py, "_pool")) {
                if !class_pool.is_none() {
                    class_pool.call_method0(intern!(py, "disconnect"))?;
                    redis_type.setattr(intern!(py, "_pool"), py.None())?;
                }
            }
        }
        Ok(())
    }

    #[pyo3(name = "mset", signature = (*args, **kwargs))]
    fn mset(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let redis_module = py.import_bound(intern!(py, "redis"))?;
        let redis_error = redis_module.getattr(intern!(py, "RedisError"))?;

        let mapping = PyDict::new_bound(py);

        if args.len() > 1 {
            let error =
                redis_error.call1((intern!(py, "MSET requires **kwargs or a single dict arg"),))?;
            return Err(PyErr::from_value_bound(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0)?.extract::<Bound<'_, PyDict>>() else {
                let error = redis_error
                    .call1((intern!(py, "MSET requires **kwargs or a single dict arg"),))?;
                return Err(PyErr::from_value_bound(error));
            };
            mapping.update(dict_arg.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = Self::get_redis(slf_, py)?;
        redis_connection.call_method1(py, intern!(py, "mset"), (mapping,))
    }

    #[pyo3(name = "msetnx", signature = (*args, **kwargs))]
    fn msetnx(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let redis_module = py.import_bound(intern!(py, "redis"))?;
        let redis_error = redis_module.getattr(intern!(py, "RedisError"))?;

        let mapping = PyDict::new_bound(py);

        if args.len() > 1 {
            let error = redis_error
                .call1((intern!(py, "MSENXT requires **kwargs or a single dict arg"),))?;
            return Err(PyErr::from_value_bound(error));
        }

        if args.len() == 1 {
            let Ok(dict_arg) = args.get_item(0)?.extract::<Bound<'_, PyDict>>() else {
                let error = redis_error
                    .call1((intern!(py, "MSETNX requires **kwargs or a single dict arg"),))?;
                return Err(PyErr::from_value_bound(error));
            };
            mapping.update(dict_arg.as_mapping())?;
        }

        if let Some(kwargs_dict) = kwargs {
            mapping.update(kwargs_dict.as_mapping())?;
        }

        let redis_connection = Self::get_redis(slf_, py)?;
        redis_connection.call_method1(py, intern!(py, "msetnx"), (mapping,))
    }

    #[pyo3(name = "zadd", signature = (*args, **kwargs))]
    fn zadd(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let redis_connection = Self::get_redis(slf_, py)?;

        if args.len() == 1 && args.get_item(0)?.extract::<Bound<'_, PyDict>>().is_ok() {
            return redis_connection.call_method_bound(py, intern!(py, "zadd"), args, kwargs);
        }

        let redis_module = py.import_bound(intern!(py, "redis"))?;
        let redis_error = redis_module.getattr(intern!(py, "RedisError"))?;

        if args.len() % 2 != 0 {
            let error = redis_error.call1((intern!(
                py,
                "ZADD requires an equal number of values and scores"
            ),))?;
            return Err(PyErr::from_value_bound(error));
        }
        let pieces: Vec<(String, String)> =
            args.iter().map(|item| item.to_string()).tuples().collect();

        redis_connection.call_method_bound(py, intern!(py, "zadd"), (pieces,), kwargs)
    }

    fn zincrby(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        name: &str,
        amount: Bound<'_, PyAny>,
        value: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        let redis_connection = Self::get_redis(slf_, py)?;

        let (real_value, real_amount) =
            if value.is_instance_of::<PyFloat>() || value.is_instance_of::<PyInt>() {
                (amount, value)
            } else {
                (value, amount)
            };

        redis_connection.call_method1(py, intern!(py, "zincrby"), (name, real_amount, real_value))
    }

    fn setx(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        name: &str,
        value: Bound<'_, PyAny>,
        time: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        let redis_connection = Self::get_redis(slf_, py)?;

        let (real_value, real_time) =
            if value.is_instance_of::<PyDelta>() || value.is_instance_of::<PyInt>() {
                (time, value)
            } else {
                (value, time)
            };

        redis_connection.call_method1(py, intern!(py, "setx"), (name, real_value, real_time))
    }

    fn lrem(
        slf_: &Bound<'_, Self>,
        py: Python<'_>,
        name: &str,
        value: Bound<'_, PyAny>,
        count: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        let redis_connection = Self::get_redis(slf_, py)?;

        let (real_value, real_count) = if value.is_instance_of::<PyInt>() {
            (count, value)
        } else {
            (value, count)
        };

        redis_connection.call_method1(py, intern!(py, "lrem"), (name, real_value, real_count))
    }
}

/// A subclass of :class:`shinqlx.AbstractDatabase` providing support for Redis.
#[pyclass(name = "Redis", module = "database", extends = AbstractDatabase)]
#[cfg(feature = "rust-redis")]
pub(crate) struct Redis {
    redis_client: ArcSwapOption<redis::Client>,
}
#[cfg(feature = "rust-redis")]
#[pymethods]
impl Redis {
    #[new]
    fn py_new(py: Python<'_>, plugin: PyObject) -> (Self, AbstractDatabase) {
        (
            Self {
                redis_client: Default::default(),
            },
            AbstractDatabase { plugin },
        )
    }

    fn __del__(&self, py: Python<'_>) -> PyResult<()> {
        self.close();
        Ok(())
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: bool = con.exists(key).unwrap_or(false);

            Ok(result)
        })
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<String> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let cmd = Cmd::get(key);
            let result: Option<String> = cmd
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            result.ok_or_else(|| {
                let error_msg = format!("The key '{key}' is not present in the database.");
                PyKeyError::new_err(error_msg)
            })
        })
    }

    fn __setitem__(&self, py: Python<'_>, key: &str, item: PyObject) -> PyResult<()> {
        let value = item.bind(py).str()?.to_string();

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            Cmd::set(key, value)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(())
        })
    }

    fn __delitem__(&self, py: Python<'_>, key: &str) -> PyResult<()> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            Cmd::get(key)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(())
        })
    }

    /// Sets the permission of a player.
    #[pyo3(name = "set_permission")]
    fn set_permission(&self, py: Python<'_>, player: PyObject, level: i32) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:permission", rust_player.steam_id)
        } else {
            format!("minqlx:players:{}:permission", player.bind(py).str()?)
        };

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            Cmd::set(key, level)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(())
        })
    }

    /// Gets the permission of a player.
    fn get_permission(&self, py: Python<'_>, player: PyObject) -> PyResult<i32> {
        let steam_id = if let Ok(rust_player) = player.extract::<Player>(py) {
            Ok(rust_player.steam_id)
        } else if let Ok(rust_int) = player.extract::<i64>(py) {
            Ok(rust_int)
        } else if let Ok(rust_str) = player.extract::<String>(py) {
            rust_str.parse::<i64>().map_err(|_| {
                let error_msg = format!("invalid literal for int() with base 10: '{}'", rust_str);
                PyValueError::new_err(error_msg)
            })
        } else {
            Err(PyValueError::new_err(
                "Invalid player. Use either a shinqlx.Player instance or a SteamID64.",
            ))
        }?;

        if Some(steam_id) == owner(py)? {
            return Ok(5);
        }

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let key = format!("minqlx:players:{steam_id}:permission");
            let Ok(value): RedisResult<i32> = Cmd::get(key).query(&mut con) else {
                return Ok(0);
            };

            Ok(value)
        })
    }

    /// Checks if the player has higher than or equal to *level*.
    #[pyo3(name = "has_permission", signature = (player, level=5), text_signature = "(player, level=5)")]
    #[cfg(feature = "rust-redis")]
    fn has_permission(&self, py: Python<'_>, player: PyObject, level: i32) -> PyResult<bool> {
        self.get_permission(py, player).map(|value| value >= level)
    }

    /// Sets specified player flag
    #[pyo3(name = "set_flag", signature = (player, flag, value=true), text_signature = "(player, flag, value = True)")]
    fn set_flag(&self, py: Python<'_>, player: PyObject, flag: &str, value: bool) -> PyResult<()> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.bind(py).str()?, flag)
        };

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            Cmd::set(key, if value { "1" } else { "0" })
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(())
        })
    }

    /// returns the specified player flag
    #[pyo3(name = "get_flag", signature = (player, flag, default = false), text_signature = "(player, flag, default=False)")]
    fn get_flag(
        &self,
        py: Python<'_>,
        player: PyObject,
        flag: &str,
        default: bool,
    ) -> PyResult<bool> {
        let key = if let Ok(rust_player) = player.extract::<Player>(py) {
            format!("minqlx:players:{}:flags:{}", rust_player.steam_id, flag)
        } else {
            format!("minqlx:players:{}:flags:{}", player.bind(py).str()?, flag)
        };

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let Ok(value): RedisResult<String> = Cmd::get(key).query(&mut con) else {
                return Ok(default);
            };

            Ok(!value.is_empty() && value != "0")
        })
    }

    /// Returns a connection to a Redis database. If *host* is None, it will
    /// fall back to the settings in the config and ignore the rest of the arguments.
    /// It will also share the connection across any plugins using the default
    /// configuration. Passing *host* will make it connect to a specific database
    /// that is not shared at all. Subsequent calls to this will return the connection
    /// initialized the first call unless it has been closed.
    #[pyo3(name = "connect", signature = (host = None, database = 0, unix_socket = false, password = None), text_signature = "(host = None, database = 0, unix_socket = false, password = None)")]
    fn connect(
        &self,
        py: Python<'_>,
        host: Option<&str>,
        database: i64,
        unix_socket: bool,
        password: Option<&str>,
    ) -> PyResult<()> {
        if self.redis_client.load().is_some() {
            return Ok(());
        }
        py.allow_threads(|| {
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
                    let configured_password = password_cvar.get_string();

                    let redis_url = if unix_socket_cvar {
                        if configured_password.is_empty() {
                            format!("redis+unix://{}?{}", cvar_host.get_string(), redis_db_cvar,)
                        } else {
                            format!(
                                "redis+unix://{}?{}&pass={}",
                                cvar_host.get_string(),
                                redis_db_cvar,
                                configured_password
                            )
                        }
                    } else {
                        let cvar_host_str = cvar_host.get_string();
                        let (redis_hostname, port) = cvar_host_str
                            .split_once(':')
                            .unwrap_or((&cvar_host_str, "6379"));
                        if configured_password.is_empty() {
                            format!(
                                "redis://{}:{}/{}",
                                redis_hostname,
                                if port.is_empty() { "6379" } else { port },
                                redis_db_cvar
                            )
                        } else {
                            format!(
                                "redis://:{}@{}:{}/{}",
                                configured_password,
                                redis_hostname,
                                if port.is_empty() { "6379" } else { port },
                                redis_db_cvar
                            )
                        }
                    };
                    self.redis_client.store(Some(
                        redis::Client::open(redis_url)
                            .map_err(|err| PyEnvironmentError::new_err(err.to_string()))?
                            .into(),
                    ));
                }
                Some(hostname) => {
                    let redis_url = if unix_socket {
                        match password {
                            None => {
                                format!("unix://{}?{}", hostname, database)
                            }
                            Some(pwd) => {
                                format!("unix://{}?{}&pass={}", hostname, database, pwd)
                            }
                        }
                    } else {
                        let (redis_hostname, port) =
                            hostname.split_once(':').unwrap_or((hostname, "6379"));
                        match password {
                            None => {
                                format!(
                                    "redis://{}:{}/{}",
                                    redis_hostname,
                                    if port.is_empty() { "6379" } else { port },
                                    database
                                )
                            }
                            Some(pwd) => {
                                format!(
                                    "redis://:{}@{}:{}/{}",
                                    pwd,
                                    redis_hostname,
                                    if port.is_empty() { "6379" } else { port },
                                    database
                                )
                            }
                        }
                    };
                    self.redis_client.store(Some(
                        redis::Client::open(redis_url)
                            .map_err(|err| PyEnvironmentError::new_err(err.to_string()))?
                            .into(),
                    ));
                }
            }
            Ok(())
        })
    }

    /// Close the Redis connection if the config was overridden. Otherwise only do so
    /// if this is the last plugin using the default connection.
    fn close(&self) {
        self.redis_client.store(None);
    }

    // Generic commands
    #[pyo3(name = "delete", signature = (*names))]
    fn delete(&self, py: Python<'_>, names: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let keys: Vec<String> = names
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = Cmd::del(keys)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "exists", signature = (*names))]
    fn exists(&self, py: Python<'_>, names: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let keys: Vec<String> = names
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = Cmd::exists(keys).query(&mut con).unwrap_or(0);

            Ok(result)
        })
    }

    #[pyo3(name = "keys", signature = (pattern = "*"), text_signature = "(pattern=\"*\")")]
    fn keys(&self, py: Python<'_>, pattern: &str) -> PyResult<Vec<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Vec<String> = Cmd::keys(pattern)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn scan_iter(&self, py: Python<'_>, pattern: &str) -> PyResult<ResultIter> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Vec<String> = con
                .scan_match(pattern)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
                .collect();

            let returned = ResultIter {
                iter: result.into_iter(),
            };

            Ok(returned)
        })
    }

    #[pyo3(name = "key_type")]
    fn key_type(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: String = Cmd::key_type(name)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // String commands
    #[pyo3(name = "decr", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn decr(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = Cmd::decr(name, amount)
                .query(&mut con)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "decrby", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn decrby(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<isize> {
        self.decr(py, name, amount)
    }

    fn get(&self, py: Python<'_>, name: &str) -> PyResult<Option<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<String> = con
                .get(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "incr", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn incr(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .incr(name, amount)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "incrby", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    #[cfg(feature = "rust-redis")]
    fn incrby(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<isize> {
        self.incr(py, name, amount)
    }

    #[pyo3(name = "set", signature = (name, value, ex = None, px = None, nx = false, xx = false), text_signature = "(name, value, ex=None, px=None, nx=False, xx=False)")]
    #[allow(clippy::too_many_arguments)]
    fn set(
        &self,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        ex: Option<usize>,
        px: Option<usize>,
        nx: bool,
        xx: bool,
    ) -> PyResult<Option<bool>> {
        let str_value = value.bind(py).str()?.to_string();

        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let set_options = SetOptions::default();
            if let Some(expiration) = ex {
                set_options.with_expiration(SetExpiry::EX(expiration));
            } else if let Some(expiration) = px {
                set_options.with_expiration(SetExpiry::PX(expiration));
            }
            if nx {
                set_options.conditional_set(ExistenceCheck::NX);
            } else if xx {
                set_options.conditional_set(ExistenceCheck::XX);
            }
            let result: Option<bool> = con
                .set_options(name, str_value, set_options)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // list commands
    fn lindex(&self, py: Python<'_>, name: &str, index: isize) -> PyResult<Option<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<String> = con
                .lindex(name, index)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn llen(&self, py: Python<'_>, name: &str) -> PyResult<Option<isize>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<isize> = con
                .llen(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "lpop", signature = (name, count = None), text_signature = "(name, count=None)")]
    fn lpop(
        &self,
        py: Python<'_>,
        name: &str,
        count: Option<NonZeroUsize>,
    ) -> PyResult<Option<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<String> = con
                .lpop(name, count)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "lpush", signature = (name, *values))]
    fn lpush(
        &self,
        py: Python<'_>,
        name: &str,
        values: &Bound<'_, PyTuple>,
    ) -> PyResult<Option<isize>> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<isize> = con
                .lpush(name, str_values)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn lpushx(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Option<isize>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<isize> = con
                .lpush_exists(name, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn lrange(
        &self,
        py: Python<'_>,
        name: &str,
        start: isize,
        end: isize,
    ) -> PyResult<Vec<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Vec<String> = con
                .lrange(name, start, end)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn lrem(&self, py: Python<'_>, name: &str, count: isize, value: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .lrem(name, count, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn lset(&self, py: Python<'_>, name: &str, index: isize, value: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .lset(name, index, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn ltrim(&self, py: Python<'_>, name: &str, start: isize, end: isize) -> PyResult<bool> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: bool = con
                .ltrim(name, start, end)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "rpop", signature = (name, count = None), text_signature = "(name, count=None)")]
    fn rpop(&self, py: Python<'_>, name: &str, count: Option<NonZeroUsize>) -> PyResult<String> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: String = con
                .rpop(name, count)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "rpush", signature = (name, *values))]
    fn rpush(
        &self,
        py: Python<'_>,
        name: &str,
        values: &Bound<'_, PyTuple>,
    ) -> PyResult<Option<isize>> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<isize> = con
                .rpush(name, str_values)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn rpushx(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Option<isize>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<isize> = con
                .rpush_exists(name, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // set commands
    #[pyo3(name = "sadd", signature = (name, *values))]
    fn sadd(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .sadd(name, str_values)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn scard(&self, py: Python<'_>, name: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .scard(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn sismember(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<bool> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: bool = con
                .sismember(name, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn smembers(&self, py: Python<'_>, name: &str) -> PyResult<Py<PySet>> {
        self.connect(py, None, 0, false, None)?;
        let result = py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let res: Vec<String> = con
                .smembers(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            Ok(res)
        })?;
        let returned = PySet::new_bound(py, result.iter())?;

        Ok(returned.unbind())
    }

    #[pyo3(name = "srem", signature = (name, *values))]
    fn srem(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .srem(name, str_values)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // hash commands
    #[pyo3(name = "hdel", signature = (name, *keys))]
    fn hdel(&self, py: Python<'_>, name: &str, keys: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let str_keys: Vec<String> = keys
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .hdel(name, str_keys)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hexists(&self, py: Python<'_>, name: &str, key: &str) -> PyResult<bool> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: bool = con
                .hexists(name, key)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hget(&self, py: Python<'_>, name: &str, key: &str) -> PyResult<Option<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<String> = con
                .hget(name, key)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hgetall(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyDict>> {
        self.connect(py, None, 0, false, None)?;
        let result = py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let res: Vec<(String, String)> = con
                .hgetall(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(res)
        })?;

        let returned = PyDict::new_bound(py);
        for (key, value) in result {
            returned.set_item(key, value)?;
        }

        Ok(returned.unbind())
    }

    #[pyo3(name = "hincrby", signature = (name, key, amount = 1), text_signature = "(name, key, amount=1)")]
    fn hincrby(&self, py: Python<'_>, name: &str, key: &str, amount: isize) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .hincr(name, key, amount)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hkeys(&self, py: Python<'_>, name: &str) -> PyResult<Vec<String>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Vec<String> = con
                .hkeys(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hlen(&self, py: Python<'_>, name: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .hlen(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hset(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .hset(name, key, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hsetnx(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .hset_nx(name, key, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn hmset(&self, py: Python<'_>, name: &str, mapping: &Bound<'_, PyMapping>) -> PyResult<bool> {
        let values: Vec<(String, String)> = mapping
            .items()?
            .iter()?
            .filter_map(|sequence| {
                sequence
                    .ok()
                    .and_then(|item| item.extract::<(String, String)>().ok())
            })
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: bool = con
                .hset_multiple(name, values.as_slice())
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name="hmget", signature = (name, *keys))]
    fn hmget(
        &self,
        py: Python<'_>,
        name: &str,
        keys: &Bound<'_, PyTuple>,
    ) -> PyResult<Vec<Option<String>>> {
        let str_keys: Vec<String> = keys
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Vec<Option<String>> = con
                .hget(name, str_keys)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // sorted set commands
    #[pyo3(name="zadd", signature = (name, *args, **kwargs))]
    fn zadd(
        &self,
        py: Python<'_>,
        name: &str,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<isize> {
        if args.len() % 2 != 0 {
            return Err(PyValueError::new_err(
                "ZADD requires an equal number of values and scores",
            ));
        }
        let mut pieces: Vec<(String, String)> =
            args.iter().map(|item| item.to_string()).tuples().collect();
        if let Some(keyword_arguments) = kwargs {
            let kwargs_pieces: Vec<(String, String)> = keyword_arguments
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            pieces.extend(kwargs_pieces);
        }
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .zadd_multiple(name, pieces.as_slice())
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn zcard(&self, py: Python<'_>, name: &str) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .zcard(name)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn zincrby(&self, py: Python<'_>, name: &str, value: &str, amount: f64) -> PyResult<f64> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: f64 = con
                .zincr(name, value, amount)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "zrange", signature = (name, start, end, *, withscores = false), text_signature = "(name, start, end, *, withscores=False)")]
    fn zrange(
        &self,
        py: Python<'_>,
        name: &str,
        start: isize,
        end: isize,
        withscores: bool,
    ) -> PyResult<PyObject> {
        self.connect(py, None, 0, false, None)?;
        let Some(ref client) = *self.redis_client.load() else {
            return Err(PyEnvironmentError::new_err(
                "could not get redis connection.",
            ));
        };
        let mut con = client
            .get_connection()
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        if withscores {
            let result: Vec<(String, f64)> = con
                .zrange_withscores(name, start, end)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            return Ok(result.into_py(py));
        }

        let result: Vec<String> = con
            .zrange(name, start, end)
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        Ok(result.into_py(py))
    }

    #[pyo3(name = "zrangebyscore", signature = (name, min, max, start = None, num = None, *, withscores = false), text_signature = "(name, min, max, start=None, num=None, *, withscores=False)")]
    #[allow(clippy::too_many_arguments)]
    fn zrangebyscore(
        &self,
        py: Python<'_>,
        name: &str,
        min: PyObject,
        max: PyObject,
        start: Option<isize>,
        num: Option<isize>,
        withscores: bool,
    ) -> PyResult<PyObject> {
        let min_parm = if let Ok(value) = min.extract::<f64>(py) {
            Ok(format!("{value}"))
        } else {
            min.extract::<String>(py)
        }?;
        let max_parm = if let Ok(value) = max.extract::<f64>(py) {
            Ok(format!("{value}"))
        } else {
            max.extract::<String>(py)
        }?;
        self.connect(py, None, 0, false, None)?;
        let Some(ref client) = *self.redis_client.load() else {
            return Err(PyEnvironmentError::new_err(
                "could not get redis connection.",
            ));
        };
        let mut con = client
            .get_connection()
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        if withscores {
            let result: Vec<(String, f64)> = if start.is_some() || num.is_some() {
                con.zrangebyscore_limit_withscores(
                    name,
                    min_parm,
                    max_parm,
                    start.unwrap_or(0),
                    num.unwrap_or(-1),
                )
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
            } else {
                con.zrangebyscore_withscores(name, min_parm, max_parm)
                    .map_err(|err| PyConnectionError::new_err(err.to_string()))?
            };
            return Ok(result.into_py(py));
        }

        let result: Vec<String> = if start.is_some() || num.is_some() {
            con.zrangebyscore_limit(
                name,
                min_parm,
                max_parm,
                start.unwrap_or(0),
                num.unwrap_or(-1),
            )
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?
        } else {
            con.zrangebyscore(name, min_parm, max_parm)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
        };

        Ok(result.into_py(py))
    }

    #[pyo3(name = "zrem", signature = (name, *values))]
    fn zrem(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<isize> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .zrem(name, str_values)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    fn zremrangebyscore(&self, py: Python<'_>, name: &str, min: f64, max: f64) -> PyResult<isize> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: isize = con
                .zrembyscore(name, min, max)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    #[pyo3(name = "zrevrange", signature = (name, start, end, *, withscores = false), text_signature = "(name, start, end, *, withscores=False)")]
    fn zrevrange(
        &self,
        py: Python<'_>,
        name: &str,
        start: isize,
        end: isize,
        withscores: bool,
    ) -> PyResult<PyObject> {
        self.connect(py, None, 0, false, None)?;
        let Some(ref client) = *self.redis_client.load() else {
            return Err(PyEnvironmentError::new_err(
                "could not get redis connection.",
            ));
        };
        let mut con = client
            .get_connection()
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        if withscores {
            let result: Vec<(String, f64)> = con
                .zrevrange_withscores(name, start, end)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            return Ok(result.into_py(py));
        }

        let result: Vec<String> = con
            .zrevrange(name, start, end)
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        Ok(result.into_py(py))
    }

    #[pyo3(name = "zrevrangebyscore", signature = (name, min, max, start = None, num = None, *, withscores = false), text_signature = "(name, min, max, start=None, num=None, *, withscores=False)")]
    #[allow(clippy::too_many_arguments)]
    fn zrevrangebyscore(
        &self,
        py: Python<'_>,
        name: &str,
        min: &str,
        max: &str,
        start: Option<isize>,
        num: Option<isize>,
        withscores: bool,
    ) -> PyResult<PyObject> {
        self.connect(py, None, 0, false, None)?;
        let Some(ref client) = *self.redis_client.load() else {
            return Err(PyEnvironmentError::new_err(
                "could not get redis connection.",
            ));
        };
        let mut con = client
            .get_connection()
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

        if withscores {
            let result: Vec<(String, f64)> = if start.is_some() || num.is_some() {
                con.zrevrangebyscore_limit_withscores(
                    name,
                    min,
                    max,
                    start.unwrap_or(0),
                    num.unwrap_or(-1),
                )
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
            } else {
                con.zrevrangebyscore_withscores(name, min, max)
                    .map_err(|err| PyConnectionError::new_err(err.to_string()))?
            };
            return Ok(result.into_py(py));
        }

        let result: Vec<String> = if start.is_some() || num.is_some() {
            con.zrevrangebyscore_limit(name, min, max, start.unwrap_or(0), num.unwrap_or(-1))
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
        } else {
            con.zrevrangebyscore(name, min, max)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?
        };

        Ok(result.into_py(py))
    }

    fn zscore(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Option<f64>> {
        self.connect(py, None, 0, false, None)?;
        py.allow_threads(|| {
            let Some(ref client) = *self.redis_client.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get redis connection.",
                ));
            };
            let mut con = client
                .get_connection()
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
            let result: Option<f64> = con
                .zscore(name, value)
                .map_err(|err| PyConnectionError::new_err(err.to_string()))?;

            Ok(result)
        })
    }

    // pipeline
    fn pipeline(slf_: Bound<'_, Self>) -> PyResult<RedisPipeline> {
        Ok(RedisPipeline {
            redis_client: slf_.unbind(),
            pipeline: parking_lot::RwLock::new(Pipeline::new()),
        })
    }
}

#[cfg(feature = "rust-redis")]
#[pyclass]
struct ResultIter {
    iter: IntoIter<String>,
}

#[cfg(feature = "rust-redis")]
#[pymethods]
impl ResultIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<String> {
        slf.iter.next()
    }
}

#[cfg(feature = "rust-redis")]
#[pyclass]
struct RedisPipeline {
    redis_client: Py<Redis>,
    pipeline: parking_lot::RwLock<Pipeline>,
}

#[cfg(feature = "rust-redis")]
#[pymethods]
impl RedisPipeline {
    fn execute(&self, py: Python<'_>) -> PyResult<()> {
        self.redis_client
            .borrow(py)
            .connect(py, None, 0, false, None)?;
        let Some(ref client) = *self.redis_client.borrow(py).redis_client.load() else {
            return Err(PyEnvironmentError::new_err(
                "could not get redis connection.",
            ));
        };
        let mut con = client
            .get_connection()
            .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
        self.pipeline.try_read().map_or(
            Err(PyConnectionError::new_err("could not access pipeline.")),
            |ref pipeline| {
                pipeline
                    .query(&mut con)
                    .map_err(|err| PyConnectionError::new_err(err.to_string()))?;
                Ok(())
            },
        )
    }

    // Generic commands
    #[pyo3(name = "delete", signature = (*names))]
    fn delete(&self, py: Python<'_>, names: &Bound<'_, PyTuple>) -> PyResult<()> {
        let keys: Vec<String> = names
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.del(keys);
                    Ok(())
                },
            )
        })
    }

    // String commands
    #[pyo3(name = "decr", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn decr(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.decr(name, amount);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "decrby", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn decrby(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<()> {
        self.decr(py, name, amount)
    }

    #[pyo3(name = "incr", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn incr(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.incr(name, amount);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "incrby", signature = (name, amount = 1), text_signature = "(name, amount=1)")]
    fn incrby(&self, py: Python<'_>, name: &str, amount: isize) -> PyResult<()> {
        self.incr(py, name, amount)
    }

    #[pyo3(name = "set", signature = (name, value, ex = None, px = None, nx = false, xx = false), text_signature = "(name, value, ex=None, px=None, nx=False, xx=False)")]
    #[allow(clippy::too_many_arguments)]
    fn set(
        &self,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        ex: Option<usize>,
        px: Option<usize>,
        nx: bool,
        xx: bool,
    ) -> PyResult<()> {
        let str_value = value.bind(py).str()?.to_string();

        py.allow_threads(|| {
            let set_options = SetOptions::default();
            if let Some(expiration) = ex {
                set_options.with_expiration(SetExpiry::EX(expiration));
            } else if let Some(expiration) = px {
                set_options.with_expiration(SetExpiry::PX(expiration));
            }
            if nx {
                set_options.conditional_set(ExistenceCheck::NX);
            } else if xx {
                set_options.conditional_set(ExistenceCheck::XX);
            }
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.set_options(name, str_value, set_options);
                    Ok(())
                },
            )
        })
    }

    // list commands
    #[pyo3(name = "lpop", signature = (name, count = None), text_signature = "(name, count=None)")]
    fn lpop(&self, py: Python<'_>, name: &str, count: Option<NonZeroUsize>) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.lpop(name, count);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "lpush", signature = (name, *values))]
    fn lpush(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.lpush(name, str_values);
                    Ok(())
                },
            )
        })
    }

    fn lpushx(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.lpush_exists(name, value);
                    Ok(())
                },
            )
        })
    }

    fn lrem(&self, py: Python<'_>, name: &str, count: isize, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.lrem(name, count, value);
                    Ok(())
                },
            )
        })
    }

    fn lset(&self, py: Python<'_>, name: &str, index: isize, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.lset(name, index, value);
                    Ok(())
                },
            )
        })
    }

    fn ltrim(&self, py: Python<'_>, name: &str, start: isize, end: isize) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.ltrim(name, start, end);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "rpop", signature = (name, count = None), text_signature = "(name, count=None)")]
    fn rpop(&self, py: Python<'_>, name: &str, count: Option<NonZeroUsize>) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.rpop(name, count);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "rpush", signature = (name, * values))]
    fn rpush(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.rpush(name, str_values);
                    Ok(())
                },
            )
        })
    }

    fn rpushx(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.rpush_exists(name, value);
                    Ok(())
                },
            )
        })
    }

    // set commands
    #[pyo3(name = "sadd", signature = (name, *values))]
    fn sadd(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.sadd(name, str_values);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "srem", signature = (name, *values))]
    fn srem(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.srem(name, str_values);
                    Ok(())
                },
            )
        })
    }

    // hash commands
    #[pyo3(name = "hdel", signature = (name, *keys))]
    fn hdel(&self, py: Python<'_>, name: &str, keys: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_keys: Vec<String> = keys
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.hdel(name, str_keys);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "hincrby", signature = (name, key, amount = 1), text_signature = "(name, key, amount=1)")]
    fn hincrby(&self, py: Python<'_>, name: &str, key: &str, amount: isize) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.hincr(name, key, amount);
                    Ok(())
                },
            )
        })
    }

    fn hset(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.hset(name, key, value);
                    Ok(())
                },
            )
        })
    }

    fn hsetnx(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.hset_nx(name, key, value);
                    Ok(())
                },
            )
        })
    }

    fn hmset(&self, py: Python<'_>, name: &str, mapping: &Bound<'_, PyMapping>) -> PyResult<()> {
        let values: Vec<(String, String)> = mapping
            .items()?
            .iter()?
            .filter_map(|sequence| {
                sequence
                    .ok()
                    .and_then(|item| item.extract::<(String, String)>().ok())
            })
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.hset_multiple(name, values.as_slice());
                    Ok(())
                },
            )
        })
    }

    // sorted set commands
    #[pyo3(name = "zadd", signature = (name, *args, **kwargs))]
    fn zadd(
        &self,
        py: Python<'_>,
        name: &str,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        if args.len() % 2 != 0 {
            return Err(PyValueError::new_err(
                "ZADD requires an equal number of values and scores",
            ));
        }
        let mut pieces: Vec<(String, String)> =
            args.iter().map(|item| item.to_string()).tuples().collect();
        if let Some(keyword_arguments) = kwargs {
            let kwargs_pieces: Vec<(String, String)> = keyword_arguments
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            pieces.extend(kwargs_pieces);
        }
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.zadd_multiple(name, pieces.as_slice());
                    Ok(())
                },
            )
        })
    }

    fn zincrby(&self, py: Python<'_>, name: &str, value: &str, amount: f64) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.zincr(name, value, amount);
                    Ok(())
                },
            )
        })
    }

    #[pyo3(name = "zrem", signature = (name, * values))]
    fn zrem(&self, py: Python<'_>, name: &str, values: &Bound<'_, PyTuple>) -> PyResult<()> {
        let str_values: Vec<String> = values
            .iter()
            .filter_map(|value| value.str().ok().map(|str_value| str_value.to_string()))
            .collect();
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.zrem(name, str_values);
                    Ok(())
                },
            )
        })
    }

    fn zremrangebyscore(&self, py: Python<'_>, name: &str, min: f64, max: f64) -> PyResult<()> {
        py.allow_threads(|| {
            self.pipeline.try_write().map_or(
                Err(PyConnectionError::new_err("could not access pipeline.")),
                |ref mut pipeline| {
                    pipeline.zrembyscore(name, min, max);
                    Ok(())
                },
            )
        })
    }
}
