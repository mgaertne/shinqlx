use core::{ffi::CStr, hint::cold_path};
use std::path::Path;

use log::error;
use pyo3::{
    exceptions::PyEnvironmentError,
    intern,
    types::{IntoPyDict, PyString, PyTuple},
};

use super::prelude::*;
use crate::{
    MAIN_ENGINE,
    logging::{RotationDefaults, finish_logging},
    quake_live_engine::FindCVar,
};

pub(super) fn get_logger_name(plugin: Option<Bound<'_, PyAny>>) -> String {
    plugin
        .and_then(|req_plugin| {
            req_plugin
                .str()
                .ok()
                .map(|plugin_name| plugin_name.to_string())
        })
        .map_or_else(
            || "shinqlx".to_string(),
            |plugin_name| format!("shinqlx.{plugin_name}"),
        )
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

/// Writes a debug record for the target `shinqlx` straight into the log, without going through the
/// Python logging. It is skipped before anything is formatted when no appender takes debug records.
pub(crate) fn log_debug(func: &str, message: &str) {
    if !log::log_enabled!(target: "shinqlx", log::Level::Debug) {
        return;
    }
    log::logger().log(
        &log::Record::builder()
            .level(log::Level::Debug)
            .target("shinqlx")
            .args(format_args!("{message}"))
            .key_values(&[("func", func)].as_slice())
            .build(),
    );
}

const HANDLER_SOURCE: &CStr = c"
import logging


class ShinqlxHandler(logging.Handler):
    def __init__(self, emit_record):
        super().__init__(logging.DEBUG)
        self._emit_record = emit_record
        self.setFormatter(logging.Formatter('%(message)s'))

    def emit(self, record):
        try:
            self._emit_record(record.name, record.levelno, self.format(record), record.funcName)
        except Exception:
            self.handleError(record)
";

fn log_level_of(level_number: i32) -> log::Level {
    match level_number {
        40.. => log::Level::Error,
        30..40 => log::Level::Warn,
        20..30 => log::Level::Info,
        10..20 => log::Level::Debug,
        _ => log::Level::Trace,
    }
}

/// Hands a record of the Python logging over to the Rust logging. The `.` of the Python logger
/// name becomes `::`, which is how the loggers of the log configuration are nested.
#[pyfunction]
#[pyo3(name = "_log_record")]
fn pyshinqlx_log_record(name: &str, level_number: i32, message: &str, func: &str) {
    let target = name.replace('.', "::");
    log::logger().log(
        &log::Record::builder()
            .level(log_level_of(level_number))
            .target(&target)
            .args(format_args!("{message}"))
            .key_values(&[("func", func)].as_slice())
            .build(),
    );
}

#[pyfunction]
#[pyo3(name = "_configure_logger")]
pub(super) fn pyshinqlx_configure_logger(py: Python<'_>) -> PyResult<()> {
    let (homepath, num_max_logs, max_logssize) = MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PyEnvironmentError::new_err("no main engine found"))
        },
        |main_engine| {
            let homepath = main_engine
                .find_cvar("fs_homepath")
                .map(|homepath_cvar| homepath_cvar.get_string().to_string())
                .unwrap_or_else(|| ".".into());
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

    finish_logging(
        Path::new(&homepath),
        RotationDefaults {
            max_size: u64::try_from(max_logssize).unwrap_or_default(),
            count: u32::try_from(num_max_logs).unwrap_or_default(),
        },
    )
    .iter()
    .for_each(
        |problem| error!(target: "shinqlx", func = "pyshinqlx_configure_logger"; "{problem}"),
    );

    let logging_module = py.import(intern!(py, "logging"))?;
    let handler = PyModule::from_code(
        py,
        HANDLER_SOURCE,
        c"shinqlx_log_handler.py",
        c"shinqlx_log_handler",
    )?
    .getattr(intern!(py, "ShinqlxHandler"))?
    .call1((wrap_pyfunction!(pyshinqlx_log_record, py)?,))?;

    let shinqlx_logger =
        logging_module.call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))?;
    shinqlx_logger.call_method1(
        intern!(py, "setLevel"),
        (logging_module.getattr(intern!(py, "DEBUG"))?,),
    )?;
    shinqlx_logger.call_method1(intern!(py, "addHandler"), (handler,))?;

    let datetime_now = py
        .import(intern!(py, "datetime"))?
        .getattr(intern!(py, "datetime"))?
        .call_method0(intern!(py, "now"))?;
    shinqlx_logger
        .call_method1(
            intern!(py, "info"),
            (
                "============================= shinqlx run @ %s =============================",
                datetime_now,
            ),
        )
        .map(|_| ())
}

#[cfg(test)]
mod pyshinqlx_configure_logger_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use pretty_assertions::assert_eq;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*};
    use rstest::rstest;

    use super::{log_level_of, pyshinqlx_configure_logger};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::pyshinqlx_setup_fixture::*,
        },
        prelude::*,
    };

    fn clear_handlers(py: Python<'_>) {
        let shinqlx_logger = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| {
                logging_module.call_method1(intern!(py, "getLogger"), ("shinqlx",))
            })
            .expect("this should not happen");
        shinqlx_logger
            .getattr(intern!(py, "handlers"))
            .and_then(|handlers| handlers.call_method0(intern!(py, "clear")))
            .expect("this should not happen");
    }

    #[rstest]
    #[case(50, log::Level::Error)]
    #[case(40, log::Level::Error)]
    #[case(30, log::Level::Warn)]
    #[case(20, log::Level::Info)]
    #[case(10, log::Level::Debug)]
    #[case(5, log::Level::Trace)]
    #[case(0, log::Level::Trace)]
    fn python_levels_map_to_log_levels(#[case] number: i32, #[case] expected: log::Level) {
        assert_eq!(log_level_of(number), expected);
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
                    clear_handlers(py);

                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok(), "{result:?}");

                    let logging_module = py
                        .import(intern!(py, "logging"))
                        .expect("this should not happen");
                    let shinqlx_logger = logging_module
                        .call_method1(intern!(py, "getLogger"), ("shinqlx",))
                        .expect("this should not happen");
                    let log_level = shinqlx_logger
                        .getattr(intern!(py, "level"))
                        .and_then(|level| level.extract::<i32>())
                        .expect("this should not happen");
                    assert_eq!(log_level, 10);

                    let handler_names = shinqlx_logger
                        .getattr(intern!(py, "handlers"))
                        .and_then(|handlers| handlers.extract::<Vec<Bound<'_, PyAny>>>())
                        .expect("this should not happen")
                        .iter()
                        .map(|handler| {
                            handler
                                .get_type()
                                .name()
                                .expect("this should not happen")
                                .to_string()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(handler_names, vec!["ShinqlxHandler"]);

                    clear_handlers(py);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configure_logger_with_no_cvars_configured_in_main_engine(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "fs_homepath", |_| None, 1)
            .with_find_cvar(|cmd| cmd == "qlx_logs", |_| None, 1)
            .with_find_cvar(|cmd| cmd == "qlx_logsSize", |_| None, 1)
            .run(|| {
                Python::attach(|py| {
                    clear_handlers(py);

                    let result = pyshinqlx_configure_logger(py);
                    assert!(result.is_ok(), "{result:?}");

                    clear_handlers(py);
                });
            });
    }
}

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
    #[serial_test::serial]
    fn log_exception_with_no_exception_pending(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_log_exception(py, None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial_test::serial]
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
    #[serial_test::serial]
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
    #[serial_test::serial]
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
    #[serial_test::serial]
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
