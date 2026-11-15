use std::path::{Path, PathBuf};

use anyhow::Context;
use log4rs::{
    append::{
        Append,
        rolling_file::{
            RollingFileAppender,
            policy::compound::{
                CompoundPolicy,
                roll::{delete::DeleteRoller, fixed_window::FixedWindowRoller},
                trigger::size::SizeTrigger,
            },
        },
    },
    config::{Deserialize, Deserializers},
};
use parking_lot::Mutex;
use serde::Deserialize as SerdeDeserialize;

use super::{AsyncAppender, DottedPatternEncoder};

pub(super) const FILE_PATTERN: &str = "\\({d(%H:%M:%S)}\\) [{l} @ {t}.{K(func)}] {m}{n}";
pub(super) const DEFAULT_FILE_NAME: &str = "shinqlx.log";

/// Where and how a log file is written; values that are not set come from the cvars
/// `qlx_logsSize` and `qlx_logs` when the file is attached.
#[derive(Debug, Clone, Default, SerdeDeserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LogFileSpec {
    pub(super) path: Option<String>,
    pub(super) max_size: Option<u64>,
    pub(super) count: Option<u32>,
    pub(super) pattern: Option<String>,
}

/// The limits of the cvars `qlx_logsSize` and `qlx_logs`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RotationDefaults {
    pub(crate) max_size: u64,
    pub(crate) count: u32,
}

struct PendingFile {
    appender: AsyncAppender,
    spec: LogFileSpec,
}

static PENDING_FILES: Mutex<Vec<PendingFile>> = Mutex::new(Vec::new());

/// Forgets the files that were not attached, for example when their configuration is replaced.
pub(super) fn clear_pending_files() {
    PENDING_FILES.lock().clear();
}

/// Creates the appender for a log file. Its records are held back until [`attach_log_files`] opens
/// the file, because the directory of the files is only known once the engine runs.
pub(super) fn pending_log_file(spec: LogFileSpec) -> AsyncAppender {
    let appender = AsyncAppender::spawn();
    PENDING_FILES.lock().push(PendingFile {
        appender: appender.clone(),
        spec,
    });
    appender
}

/// Opens all log files that are still pending below `directory`, relative paths are resolved
/// against it. Returns the files that could not be opened.
pub(crate) fn attach_log_files(
    directory: &Path,
    defaults: RotationDefaults,
) -> Vec<(PathBuf, anyhow::Error)> {
    PENDING_FILES
        .lock()
        .drain(..)
        .filter_map(|pending| {
            let path = directory.join(pending.spec.path.as_deref().unwrap_or(DEFAULT_FILE_NAME));
            match open_rolling_file(&path, &pending.spec, defaults) {
                Ok(file) => {
                    pending.appender.attach(file);
                    None
                }
                Err(error) => Some((path, error)),
            }
        })
        .collect()
}

fn open_rolling_file(
    path: &Path,
    spec: &LogFileSpec,
    defaults: RotationDefaults,
) -> anyhow::Result<Box<dyn Append>> {
    let max_size = match spec.max_size.unwrap_or(defaults.max_size) {
        0 => u64::MAX,
        size => size,
    };
    let count = spec.count.unwrap_or(defaults.count);
    let roller: Box<dyn log4rs::append::rolling_file::policy::compound::roll::Roll> = if count == 0
    {
        Box::new(DeleteRoller::new())
    } else {
        Box::new(
            FixedWindowRoller::builder()
                .base(1)
                .build(&format!("{}.{{}}", path.display()), count)?,
        )
    };
    let policy = CompoundPolicy::new(Box::new(SizeTrigger::new(max_size)), roller);
    let pattern = spec.pattern.as_deref().unwrap_or(FILE_PATTERN);
    RollingFileAppender::builder()
        .encoder(Box::new(DottedPatternEncoder::new(pattern)))
        .build(path, Box::new(policy))
        .map(|appender| Box::new(appender) as Box<dyn Append>)
        .with_context(|| format!("could not open the log file {}", path.display()))
}

pub(super) struct LogFileDeserializer;

impl Deserialize for LogFileDeserializer {
    type Config = LogFileSpec;
    type Trait = dyn Append;

    fn deserialize(
        &self,
        spec: LogFileSpec,
        _deserializers: &Deserializers,
    ) -> anyhow::Result<Box<dyn Append>> {
        Ok(Box::new(pending_log_file(spec)))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use log::{Level, Record};
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    use super::*;

    fn write_through(appender: &AsyncAppender, message: &str) {
        appender
            .append(
                &Record::builder()
                    .level(Level::Info)
                    .target("shinqlx::plugin")
                    .args(format_args!("{message}"))
                    .key_values(&[("func", "handler")].as_slice())
                    .build(),
            )
            .unwrap();
    }

    #[test]
    #[serial]
    fn records_written_before_the_attach_end_up_in_the_file_below_the_directory() {
        PENDING_FILES.lock().clear();
        let directory = tempfile_dir("attach");
        let appender = pending_log_file(LogFileSpec::default());
        write_through(&appender, "early");

        let failures = attach_log_files(
            &directory,
            RotationDefaults {
                max_size: 3_000_000,
                count: 2,
            },
        );
        write_through(&appender, "late");
        appender.wait_until_written(core::time::Duration::from_secs(5));

        assert!(failures.is_empty(), "{failures:?}");
        let content = fs::read_to_string(directory.join("shinqlx.log")).unwrap();
        eprintln!("CONTENT: {content:?}");
        let lines: Vec<&str> = content
            .lines()
            .map(|line| line.split_once(") ").unwrap().1)
            .collect();
        assert_eq!(
            lines,
            vec![
                "[INFO @ shinqlx.plugin.handler] early",
                "[INFO @ shinqlx.plugin.handler] late"
            ]
        );
    }

    #[test]
    #[serial]
    fn the_file_rotates_like_the_python_rotating_file_handler() {
        PENDING_FILES.lock().clear();
        let directory = tempfile_dir("rotate");
        let appender = pending_log_file(LogFileSpec::default());
        attach_log_files(
            &directory,
            RotationDefaults {
                max_size: 200,
                count: 2,
            },
        );

        (0..30).for_each(|i| write_through(&appender, &format!("line number {i:02}")));
        appender.wait_until_written(core::time::Duration::from_secs(5));

        let mut names: Vec<String> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert!(
            names.starts_with(&["shinqlx.log.1".to_owned(), "shinqlx.log.2".to_owned()])
                || names == ["shinqlx.log", "shinqlx.log.1", "shinqlx.log.2"],
            "{names:?}"
        );
        assert!(!names.contains(&"shinqlx.log.3".to_owned()), "{names:?}");
    }

    #[test]
    #[serial]
    fn a_directory_that_cannot_be_written_is_reported() {
        PENDING_FILES.lock().clear();
        let _appender = pending_log_file(LogFileSpec {
            path: Some("no/such/dir/is/created/..".to_owned()),
            ..LogFileSpec::default()
        });

        let failures = attach_log_files(
            Path::new("/proc/shinqlx-does-not-exist"),
            RotationDefaults {
                max_size: 1000,
                count: 1,
            },
        );

        assert_eq!(failures.len(), 1);
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("shinqlx-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}
