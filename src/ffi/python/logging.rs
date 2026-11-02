use core::hint::cold_path;

use pyo3::{
    exceptions::PyEnvironmentError,
    intern,
    sync::PyOnceLock,
    types::{IntoPyDict, PyInt, PyString, PyTuple},
};

use super::prelude::*;
use crate::{MAIN_ENGINE, quake_live_engine::FindCVar};

pub(super) fn get_logger_name(plugin: Option<Bound<'_, PyAny>>) -> String {
    plugin
        .and_then(|req_plugin| {
            req_plugin
                .str()
                .ok()
                .map(|plugin_name| plugin_name.to_string())
        })
        .map_or("shinqlx".to_string(), |plugin_name| {
            format!("shinqlx.{plugin_name}")
        })
}

/// Provides a logger that should be used by your plugin for debugging, info and error reporting. It will automatically output to both the server console as well as to a file.
#[pyfunction]
#[pyo3(name = "get_logger")]
#[pyo3(signature = (plugin = None), text_signature = "(plugin = None)")]
pub(crate) fn pyshinqlx_get_logger<'py>(
    py: Python<'py>,
    plugin: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let logger_name = get_logger_name(plugin);
    PyModule::import(py, intern!(py, "logging")).and_then(|logging_module| {
        logging_module.call_method1(intern!(py, "getLogger"), (logger_name,))
    })
}

static SHINQLX_LOGGER: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static DEBUG_LEVEL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// Writes a debug record to the `shinqlx` logger. Like `logger.debug()` it ends up in all handlers
/// that accept debug records, but it neither looks up the caller of the log call nor the logger
/// and the log level on every call.
pub(crate) fn log_debug(py: Python<'_>, func: &str, message: &str) -> PyResult<()> {
    let logger = SHINQLX_LOGGER
        .get_or_try_init(py, || {
            py.import(intern!(py, "logging"))
                .and_then(|logging_module| {
                    logging_module.call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))
                })
                .map(Bound::unbind)
        })?
        .bind(py);
    let debug_level = DEBUG_LEVEL
        .get_or_try_init(py, || {
            py.import(intern!(py, "logging"))
                .and_then(|logging_module| logging_module.getattr(intern!(py, "DEBUG")))
                .map(Bound::unbind)
        })?
        .bind(py);

    let log_record = logger.call_method1(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            debug_level,
            intern!(py, ""),
            -1,
            message,
            py.None(),
            py.None(),
            PyString::intern(py, func),
        ),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    Ok(())
}

#[pyfunction]
#[pyo3(name = "_configure_logger")]
pub(super) fn pyshinqlx_configure_logger(py: Python<'_>) -> PyResult<()> {
    let (homepath, num_max_logs, max_logssize) = MAIN_ENGINE.load().as_ref().map_or(
        {
            cold_path();
            Err(PyEnvironmentError::new_err("no main engine found"))
        },
        |main_engine| {
            let homepath = main_engine
                .find_cvar("fs_homepath")
                .map(|homepath_cvar| homepath_cvar.get_string().to_string())
                .unwrap_or(".".into());
            let num_max_logs = main_engine
                .find_cvar("qlx_logs")
                .map(|max_logs_cvar| max_logs_cvar.get_integer())
                .unwrap_or_default();
            let max_logsize = main_engine
                .find_cvar("qlx_logsSize")
                .map(|max_logsize_cvar| max_logsize_cvar.get_integer())
                .unwrap_or_default();
            Ok((homepath, num_max_logs, max_logsize))
        },
    )?;

    py.import(intern!(py, "logging")).and_then(|logging_module| {
        let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
        let info_level = logging_module.getattr(intern!(py, "INFO"))?;
        logging_module
            .call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))
            .and_then(|shinqlx_logger| {
                shinqlx_logger.call_method1(intern!(py, "setLevel"), (&debug_level,))?;
                let console_fmt = logging_module.call_method1(
                    intern!(py, "Formatter"),
                    (
                        "[%(name)s.%(funcName)s] %(levelname)s: %(message)s",
                        "%H:%M:%S",
                    ),
                )?;

                logging_module
                    .call_method0(intern!(py, "StreamHandler"))
                    .and_then(|console_handler| {
                        console_handler.call_method1(intern!(py, "setLevel"), (&info_level,))?;
                        console_handler.call_method1(intern!(py, "setFormatter"), (console_fmt,))?;
                        shinqlx_logger.call_method1(intern!(py, "addHandler"), (console_handler,))
                    })?;

                let file_fmt = logging_module.call_method1(
                    intern!(py, "Formatter"),
                    (
                        "(%(asctime)s) [%(levelname)s @ %(name)s.%(funcName)s] %(message)s",
                        "%H:%M:%S",
                    ),
                )?;
                py.import(intern!(py, "logging.handlers")).and_then(|handlers_submodule| {
                    let py_max_logssize = PyInt::new(py, max_logssize).into_any();
                    let py_num_max_logs = PyInt::new(py, num_max_logs).into_any();
                    handlers_submodule
                        .call_method(
                            intern!(py, "RotatingFileHandler"),
                            (format!("{homepath}/shinqlx.log"),),
                            Some(
                                &[
                                    ("encoding", PyString::intern(py, "utf-8").into_any()),
                                    ("maxBytes", py_max_logssize),
                                    ("backupCount", py_num_max_logs),
                                ]
                                .into_py_dict(py)?,
                            ),
                        )
                        .and_then(|file_handler| {
                            file_handler.call_method1(intern!(py, "setLevel"), (&debug_level,))?;
                            file_handler.call_method1(intern!(py, "setFormatter"), (file_fmt,))?;
                            shinqlx_logger.call_method1(intern!(py, "addHandler"), (file_handler,))
                        })
                })?;

                let datetime_now = py.import(intern!(py, "datetime")).and_then(|datetime_module| {
                    datetime_module
                        .getattr(intern!(py, "datetime"))
                        .and_then(|datetime_object| datetime_object.call_method0(intern!(py, "now")))
                })?;
                shinqlx_logger
                    .call_method1(
                        intern!(py, "info"),
                        (
                            "============================= shinqlx run @ %s =============================",
                            datetime_now,
                        ),
                    )
                    .map(|_| ())
        })
    })
}

#[cfg(test)]
mod pyshinqlx_configure_logger_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use super::pyshinqlx_configure_logger;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::pyshinqlx_setup_fixture::*,
        },
        prelude::*,
    };

    fn clear_logger(py: Python<'_>) {
        let logging_module = py
            .import(intern!(py, "logging"))
            .expect("this should not happen");
        let shinqlx_logger = logging_module
            .call_method1(intern!(py, "getLogger"), ("shinqlx",))
            .expect("this should not happen");
        while shinqlx_logger
            .call_method0(intern!(py, "hasHandlers"))
            .is_ok_and(|value| {
                value
                    .cast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            })
        {
            let shinqlx_handlers = shinqlx_logger
                .getattr(intern!(py, "handlers"))
                .expect("this should not happen");
            let _ = shinqlx_logger.call_method1(
                intern!(py, "removeHandler"),
                (shinqlx_handlers
                    .get_item(0)
                    .expect("this should not happen"),),
            );
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_configure_logger(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    static TEMP_LOG_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_cvars_in_main_engine(_pyshinqlx_setup: ()) {
        let cvar_string = CString::new(TEMP_LOG_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let mut raw_homepath_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_logs_cvar = CVarBuilder::default()
            .integer(10)
            .build()
            .expect("this should not happen");
        let mut raw_logssize_cvar = CVarBuilder::default()
            .integer(1024)
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "fs_homepath",
                move |_| CVar::try_from(raw_homepath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_logs",
                move |_| CVar::try_from(raw_logs_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_logsSize",
                move |_| CVar::try_from(raw_logssize_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::attach(|py| {
                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok());

                    let logging_module = py
                        .import(intern!(py, "logging"))
                        .expect("this should not happen");
                    let debug_level = logging_module
                        .getattr(intern!(py, "DEBUG"))
                        .expect("this should not happen");
                    let shinqlx_logger = logging_module
                        .call_method1(intern!(py, "getLogger"), ("shinqlx",))
                        .expect("this should not happen");
                    let log_level = shinqlx_logger.getattr(intern!(py, "level"));
                    assert!(log_level.is_ok_and(|value| {
                        value.eq(&debug_level).expect("this should not happen")
                    }),);

                    let info_level = logging_module
                        .getattr(intern!(py, "INFO"))
                        .expect("this should not happen");
                    let shinqlx_handlers = shinqlx_logger
                        .getattr(intern!(py, "handlers"))
                        .expect("this should not happen");
                    let logging_stream_handler = logging_module
                        .getattr(intern!(py, "StreamHandler"))
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(0).is_ok_and(|first_handler| {
                        first_handler
                            .is_instance(&logging_stream_handler)
                            .expect("this should not happen")
                            && first_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&info_level).expect("this should not happen")
                                })
                    }));
                    let rotating_file_handler = logging_module
                        .getattr(intern!(py, "handlers"))
                        .and_then(|handlers_submodule| {
                            handlers_submodule.getattr(intern!(py, "RotatingFileHandler"))
                        })
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(1).is_ok_and(|second_handler| {
                        second_handler
                            .is_instance(&rotating_file_handler)
                            .expect("this should not happen")
                            && second_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&debug_level).expect("this should not happen")
                                })
                    }));

                    clear_logger(py);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_no_cvars_configured_in_main_engine(_pyshinqlx_setup: ()) {
        let cvar_string = CString::new(TEMP_LOG_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let mut raw_homepath_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "fs_homepath",
                move |_| CVar::try_from(raw_homepath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .with_find_cvar(|cmd| cmd == "qlx_logs", |_| None, 1)
            .with_find_cvar(|cmd| cmd == "qlx_logsSize", |_| None, 1)
            .run(|| {
                Python::attach(|py| {
                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok());

                    let logging_module = py
                        .import(intern!(py, "logging"))
                        .expect("this should not happen");
                    let debug_level = logging_module
                        .getattr(intern!(py, "DEBUG"))
                        .expect("this should not happen");
                    let shinqlx_logger = logging_module
                        .call_method1(intern!(py, "getLogger"), ("shinqlx",))
                        .expect("this should not happen");
                    let log_level = shinqlx_logger.getattr(intern!(py, "level"));
                    assert!(log_level.is_ok_and(|value| {
                        value.eq(&debug_level).expect("this should not happen")
                    }),);

                    let info_level = logging_module
                        .getattr(intern!(py, "INFO"))
                        .expect("this should not happen");
                    let shinqlx_handlers = shinqlx_logger
                        .getattr(intern!(py, "handlers"))
                        .expect("this should not happen");
                    let logging_stream_handler = logging_module
                        .getattr(intern!(py, "StreamHandler"))
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(0).is_ok_and(|first_handler| {
                        first_handler
                            .is_instance(&logging_stream_handler)
                            .expect("this should not happen")
                            && first_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&info_level).expect("this should not happen")
                                })
                    }));
                    let rotating_file_handler = logging_module
                        .getattr(intern!(py, "handlers"))
                        .and_then(|handlers_submodule| {
                            handlers_submodule.getattr(intern!(py, "RotatingFileHandler"))
                        })
                        .expect("this should not happen");
                    assert!(shinqlx_handlers.get_item(1).is_ok_and(|second_handler| {
                        second_handler
                            .is_instance(&rotating_file_handler)
                            .expect("this should not happen")
                            && second_handler
                                .getattr(intern!(py, "level"))
                                .is_ok_and(|log_level| {
                                    log_level.eq(&debug_level).expect("this should not happen")
                                })
                    }));

                    clear_logger(py);
                });
            });
    }
}

/// Logs an exception using :func:`get_logger`. Call this in an except block.
#[pyfunction]
#[pyo3(name = "log_exception")]
#[pyo3(signature = (plugin = None), text_signature = "(plugin = None)")]
pub(super) fn pyshinqlx_log_exception(
    py: Python<'_>,
    plugin: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    py.import(intern!(py, "sys")).and_then(|sys_module| {
        sys_module
            .call_method0(intern!(py, "exc_info"))
            .and_then(|exc_info| {
                py.import(intern!(py, "traceback"))
                    .and_then(|traceback_module| {
                        traceback_module
                            .call_method1(
                                intern!(py, "format_exception"),
                                exc_info.cast::<PyTuple>()?.to_owned(),
                            )
                            .and_then(|value| {
                                value
                                    .extract::<Vec<String>>()
                                    .and_then(|formatted_traceback| {
                                        try_log_messages(
                                            py,
                                            plugin,
                                            intern!(py, "log_exception"),
                                            formatted_traceback,
                                        )
                                    })
                            })
                    })
            })
    })
}

#[cfg(test)]
mod pyshinqlx_log_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::pyshinqlx_log_exception;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn log_exception_with_no_exception_pending(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_log_exception(py, None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn log_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

try:
    raise ValueError("asdf")
except ValueError:
    shinqlx.log_exception()
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

/// A handler for unhandled exceptions.
#[pyfunction]
#[pyo3(name = "handle_exception")]
pub(super) fn pyshinqlx_handle_exception(
    py: Python<'_>,
    exc_type: Bound<'_, PyAny>,
    exc_value: Bound<'_, PyAny>,
    exc_traceback: Bound<'_, PyAny>,
) -> PyResult<()> {
    py.import(intern!(py, "traceback"))
        .and_then(|traceback_module| {
            traceback_module
                .call_method1(
                    intern!(py, "format_exception"),
                    (exc_type, exc_value, exc_traceback),
                )
                .and_then(|value| {
                    value
                        .extract::<Vec<String>>()
                        .and_then(|formatted_traceback| {
                            try_log_messages(
                                py,
                                None,
                                intern!(py, "log_exception"),
                                formatted_traceback,
                            )
                        })
                })
        })
}

#[cfg(test)]
mod pyshinqlx_handle_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::pyshinqlx_handle_exception;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_exception_with_no_exception_pending(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_handle_exception(
                py,
                py.None().into_bound(py),
                py.None().into_bound(py),
                py.None().into_bound(py),
            );
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

try:
    raise ValueError("asdf")
except ValueError as e:
    shinqlx.handle_exception(type(e), e, e.__traceback__)
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

#[pyfunction]
#[pyo3(name = "threading_excepthook")]
pub(super) fn pyshinqlx_handle_threading_exception(
    py: Python<'_>,
    args: Bound<'_, PyAny>,
) -> PyResult<()> {
    pyshinqlx_handle_exception(
        py,
        args.getattr(intern!(py, "exc_type"))?,
        args.getattr(intern!(py, "exc_value"))?,
        args.getattr(intern!(py, "exc_traceback"))?,
    )
}

#[cfg(test)]
mod pyshinqlx_handle_threading_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_threading_exception_with_pending_exception(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

class ExceptHookArgs:
    def __init__(self, exc_type, exc_value, exc_traceback):
        self.exc_type = exc_type
        self.exc_value = exc_value
        self.exc_traceback = exc_traceback

try:
    raise ValueError("asdf")
except ValueError as e:
    shinqlx.threading_excepthook(ExceptHookArgs(type(e), e, e.__traceback__))
"#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }
}

pub(super) fn try_log_exception(
    py: Python<'_>,
    plugin: Option<Bound<'_, PyAny>>,
    exception: &PyErr,
) -> PyResult<()> {
    py.import(intern!(py, "traceback"))
        .and_then(|traceback_module| {
            traceback_module
                .call_method1(
                    intern!(py, "format_exception"),
                    (
                        exception.get_type(py),
                        exception.value(py),
                        exception.traceback(py),
                    ),
                )?
                .extract::<Vec<String>>()
                .and_then(|formatted_traceback| {
                    try_log_messages(
                        py,
                        plugin,
                        intern!(py, "log_exception"),
                        formatted_traceback,
                    )
                })
        })
}

pub(crate) fn log_exception(py: Python<'_>, exception: &PyErr) {
    let _ = try_log_exception(py, None, exception);
}

/// Logs an already caught exception under the logger of the plugin.
pub(crate) fn log_plugin_exception(
    py: Python<'_>,
    plugin: &Bound<'_, PyAny>,
    exception: &PyErr,
) -> PyResult<()> {
    try_log_exception(py, Some(plugin.clone()), exception)
}

pub(super) fn try_log_messages(
    py: Python<'_>,
    plugin: Option<Bound<'_, PyAny>>,
    function: &Bound<'_, PyString>,
    messages: Vec<String>,
) -> Result<(), PyErr> {
    py.import(intern!(py, "logging"))
        .and_then(|logging_module| {
            let error_level = logging_module.getattr(intern!(py, "ERROR"))?;
            let logger_name = get_logger_name(plugin);
            logging_module
                .call_method1(intern!(py, "getLogger"), (logger_name,))
                .and_then(|py_logger| {
                    for line in messages {
                        py_logger
                            .call_method(
                                intern!(py, "makeRecord"),
                                (
                                    intern!(py, "shinqlx"),
                                    &error_level,
                                    intern!(py, ""),
                                    -1,
                                    line.trim_end(),
                                    py.None(),
                                    py.None(),
                                ),
                                Some(&[(intern!(py, "func"), function)].into_py_dict(py)?),
                            )
                            .and_then(|log_record| {
                                py_logger.call_method1(intern!(py, "handle"), (log_record,))
                            })?;
                    }
                    Ok(())
                })
        })
}
