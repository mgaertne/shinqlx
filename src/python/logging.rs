use core::hint::cold_path;
use std::path::Path;

use chrono::Local;
use log::{error, info};
use pyo3::{exceptions::PyEnvironmentError, intern, types::PyTuple};

use super::prelude::*;
use crate::{
    MAIN_ENGINE,
    engine::FindCVar,
    logging::{RotationDefaults, finish_logging},
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

fn log_level_of(level_number: i32) -> log::Level {
    match level_number {
        40.. => log::Level::Error,
        30..40 => log::Level::Warn,
        20..30 => log::Level::Info,
        10..20 => log::Level::Debug,
        _ => log::Level::Trace,
    }
}

/// Receives the records of the Python logger `shinqlx` and hands them over to the Rust logging. The
/// `.` of the Python logger name becomes `::`, which is how the loggers of the log configuration
/// are nested. Python only needs `level` and `handle` of a handler, so no `logging.Handler` is needed.
#[pyclass(module = "_logging", name = "ShinqlxHandler", frozen)]
struct ShinqlxHandler;

impl ShinqlxHandler {
    /// The message of the record followed by its traceback and stack, as `logging.Formatter` writes it.
    fn message_of(py: Python<'_>, record: &Bound<'_, PyAny>) -> PyResult<String> {
        let mut message = record
            .call_method0(intern!(py, "getMessage"))?
            .extract::<String>()?;

        let exc_info = record.getattr(intern!(py, "exc_info"))?;
        if !exc_info.is_none() {
            let traceback = py
                .import(intern!(py, "traceback"))?
                .call_method1(
                    intern!(py, "format_exception"),
                    exc_info.cast::<PyTuple>()?.to_owned(),
                )?
                .extract::<Vec<String>>()?
                .concat();
            message.push('\n');
            message.push_str(traceback.trim_end_matches('\n'));
        }

        let stack_info = record.getattr(intern!(py, "stack_info"))?;
        if !stack_info.is_none() {
            message.push('\n');
            message.push_str(stack_info.extract::<&str>()?);
        }
        Ok(message)
    }

    fn emit(py: Python<'_>, record: &Bound<'_, PyAny>) -> PyResult<()> {
        let name = record.getattr(intern!(py, "name"))?.extract::<String>()?;
        let level_number = record.getattr(intern!(py, "levelno"))?.extract::<i32>()?;
        let func = record
            .getattr(intern!(py, "funcName"))?
            .extract::<String>()?;
        let message = Self::message_of(py, record)?;

        let target = name.replace('.', "::");
        log::logger().log(
            &log::Record::builder()
                .level(log_level_of(level_number))
                .target(&target)
                .args(format_args!("{message}"))
                .key_values(&[("func", func.as_str())].as_slice())
                .build(),
        );
        Ok(())
    }
}

#[pymethods]
impl ShinqlxHandler {
    #[getter]
    fn level(&self) -> i32 {
        10
    }

    fn handle(&self, py: Python<'_>, record: &Bound<'_, PyAny>) -> bool {
        if let Err(error) = Self::emit(py, record) {
            error.print(py);
        }
        true
    }
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
    .for_each(|problem| error!(target: "shinqlx", func = "_configure_logger"; "{problem}"));

    let logging_module = py.import(intern!(py, "logging"))?;
    let handler = Bound::new(py, ShinqlxHandler)?;

    let shinqlx_logger =
        logging_module.call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))?;
    shinqlx_logger.call_method1(
        intern!(py, "setLevel"),
        (logging_module.getattr(intern!(py, "DEBUG"))?,),
    )?;
    shinqlx_logger.call_method1(intern!(py, "addHandler"), (handler,))?;

    let datetime_now = Local::now().format("%Y-%m-%d %H:%M:%S%.6f");
    info!(
        target: "shinqlx",
        func = "_configure_logger";
        "============================= shinqlx run @ {datetime_now} ============================="
    );
    Ok(())
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
        engine::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
        python::pyshinqlx_setup_fixture::*,
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
                                value.extract::<Vec<String>>().map(|formatted_traceback| {
                                    log_error_lines(
                                        &get_logger_name(plugin),
                                        "log_exception",
                                        &formatted_traceback,
                                    )
                                })
                            })
                    })
            })
    })
}

#[cfg(test)]
mod shinqlx_handler_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::ShinqlxHandler;
    use crate::{logging::capture, python::pyshinqlx_setup_fixture::*};

    fn log_through_python_handler(
        py: Python<'_>,
        code: &core::ffi::CStr,
    ) -> Vec<(log::Level, String, String)> {
        capture::install();
        let handler = Bound::new(py, ShinqlxHandler).expect("could not create the handler");
        let logger = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| {
                logging_module.call_method1(intern!(py, "getLogger"), ("shinqlx.handler_test",))
            })
            .expect("could not get the logger");
        logger
            .setattr(intern!(py, "propagate"), false)
            .and_then(|_| logger.call_method1(intern!(py, "setLevel"), (10,)))
            .and_then(|_| logger.call_method1(intern!(py, "addHandler"), (&handler,)))
            .expect("could not attach the handler");
        let mark = capture::mark();

        let globals = pyo3::types::PyDict::new(py);
        globals
            .set_item("logger", &logger)
            .expect("could not hand over the logger");
        let result = py.run(code, Some(&globals), None);

        logger
            .call_method1(intern!(py, "removeHandler"), (&handler,))
            .expect("could not remove the handler");
        assert!(result.is_ok(), "{result:?}");
        capture::records_since(mark)
            .into_iter()
            .filter(|(_, target, _)| target == "shinqlx::handler_test")
            .collect()
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial_test::serial]
    fn python_records_reach_the_rust_logging_with_their_level_and_nested_target(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let records = log_through_python_handler(
                py,
                c"logger.debug('d %s', 1)\nlogger.warning('w')\nlogger.error('e')",
            );

            assert_eq!(
                records,
                [
                    (
                        log::Level::Debug,
                        "shinqlx::handler_test".to_owned(),
                        "d 1".to_owned()
                    ),
                    (
                        log::Level::Warn,
                        "shinqlx::handler_test".to_owned(),
                        "w".to_owned()
                    ),
                    (
                        log::Level::Error,
                        "shinqlx::handler_test".to_owned(),
                        "e".to_owned()
                    ),
                ]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial_test::serial]
    fn python_records_with_an_exception_keep_the_traceback(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let records = log_through_python_handler(
                py,
                c"try:\n    raise ValueError('asdf')\nexcept ValueError:\n    logger.exception('failed')",
            );

            assert_eq!(records.len(), 1, "{records:?}");
            assert!(records[0].2.starts_with("failed\nTraceback"), "{records:?}");
            assert!(records[0].2.contains("ValueError: asdf"), "{records:?}");
        });
    }
}

#[cfg(test)]
mod pyshinqlx_log_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::pyshinqlx_log_exception;
    use crate::python::pyshinqlx_setup_fixture::*;

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
    fn log_exception_names_the_plugin_in_the_target(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            crate::logging::capture::install();
            let mark = crate::logging::capture::mark();

            let result = py.run(
                cr#"
import shinqlx

try:
    raise ValueError("asdf")
except ValueError:
    shinqlx.log_exception("some_plugin")
"#,
                None,
                None,
            );

            assert!(result.is_ok(), "{result:?}");
            let targets = crate::logging::capture::records_since(mark)
                .into_iter()
                .map(|(_, target, _)| target)
                .collect::<Vec<_>>();
            assert!(!targets.is_empty());
            assert!(
                targets
                    .iter()
                    .all(|target| target == "shinqlx::some_plugin")
            );
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
                    value.extract::<Vec<String>>().map(|formatted_traceback| {
                        log_error_lines("shinqlx", "log_exception", &formatted_traceback)
                    })
                })
        })
}

#[cfg(test)]
mod pyshinqlx_handle_exception_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::pyshinqlx_handle_exception;
    use crate::python::pyshinqlx_setup_fixture::*;

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

    use crate::python::pyshinqlx_setup_fixture::*;

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

pub(super) fn try_log_exception(py: Python<'_>, exception: &PyErr) -> PyResult<()> {
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
                .map(|formatted_traceback| {
                    log_error_lines("shinqlx", "log_exception", &formatted_traceback)
                })
        })
}

pub(crate) fn log_exception(py: Python<'_>, exception: &PyErr) {
    let _ = try_log_exception(py, exception);
}

/// Logs the lines as errors for the Python logger `logger_name`; its `.` become `::` as in the log configuration.
pub(super) fn log_error_lines(logger_name: &str, function: &str, messages: &[String]) {
    let target = logger_name.replace('.', "::");
    messages
        .iter()
        .for_each(|line| error!(target: &target, func = function; "{}", line.trim_end()));
}
