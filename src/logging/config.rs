use std::{path::Path, sync::OnceLock};

use log::LevelFilter;
use log4rs::{
    Config, Handle,
    append::console::ConsoleAppender,
    config::{Appender, Deserializers, Root},
    filter::threshold::ThresholdFilter,
};

use super::{
    DottedPatternEncoder,
    dotted_encoder::DottedPatternEncoderDeserializer,
    log_file::{
        LogFileDeserializer, LogFileSpec, RotationDefaults, attach_log_files, clear_pending_files,
        pending_log_file,
    },
};

pub(super) const CONFIG_FILE_NAME: &str = "shinqlx_log.yml";
const CONSOLE_PATTERN: &str = "[{t}.{K(func)}] {l}: {m}{n}";

static HANDLE: OnceLock<Handle> = OnceLock::new();

pub(super) fn deserializers() -> Deserializers {
    let mut deserializers = Deserializers::default();
    deserializers.insert("shinqlx_file", LogFileDeserializer);
    deserializers.insert("shinqlx_pattern", DottedPatternEncoderDeserializer);
    deserializers
}

/// The configuration used without a `shinqlx_log.yml`; the shipped file says the same.
pub(super) fn default_config() -> Config {
    let console = ConsoleAppender::builder()
        .encoder(Box::new(DottedPatternEncoder::new(CONSOLE_PATTERN)))
        .build();
    Config::builder()
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Info)))
                .build("stdout", Box::new(console)),
        )
        .appender(
            Appender::builder().build("file", Box::new(pending_log_file(LogFileSpec::default()))),
        )
        .build(
            Root::builder()
                .appender("stdout")
                .appender("file")
                .build(LevelFilter::Debug),
        )
        .expect("the default configuration only refers to its own appenders")
}

/// The configuration of `path`, or the default one together with the reason why the file was
/// not used.
pub(super) fn config_from_file_or_default(path: &Path) -> (Config, Option<anyhow::Error>) {
    clear_pending_files();
    match log4rs::config::load_config_file(path, deserializers()) {
        Ok(config) => (config, None),
        Err(error) => {
            clear_pending_files();
            (default_config(), Some(error))
        }
    }
}

/// Starts logging with `./shinqlx_log.yml` if it exists and with the default configuration
/// otherwise. A configuration file that cannot be used is reported on stderr.
pub(crate) fn initialize_logging() {
    let path = Path::new(CONFIG_FILE_NAME);
    let (config, error) = if path.exists() {
        config_from_file_or_default(path)
    } else {
        (default_config(), None)
    };
    if let Some(error) = &error {
        eprintln!("shinqlx could not use {CONFIG_FILE_NAME}, using the defaults: {error:#}");
    }
    if let Ok(handle) = log4rs::init_config(config) {
        let _ = HANDLE.set(handle);
    }
}

/// Completes the logging once the engine knows `fs_homepath`: a `shinqlx_log.yml` in that
/// directory replaces the configuration, then the log files are opened below it. Returns what went
/// wrong so the caller can log it.
pub(crate) fn finish_logging(directory: &Path, defaults: RotationDefaults) -> Vec<String> {
    let mut problems = Vec::new();
    let homepath_config = directory.join(CONFIG_FILE_NAME);
    let is_in_working_directory = Path::new(CONFIG_FILE_NAME)
        .canonicalize()
        .ok()
        .zip(homepath_config.canonicalize().ok())
        .is_some_and(|(working, home)| working == home);
    if homepath_config.exists()
        && !is_in_working_directory
        && let Some(handle) = HANDLE.get()
    {
        let (config, error) = config_from_file_or_default(&homepath_config);
        if let Some(error) = error {
            problems.push(format!(
                "could not use {}, using the defaults: {error:#}",
                homepath_config.display()
            ));
        }
        handle.set_config(config);
    }
    problems.extend(
        attach_log_files(directory, defaults)
            .into_iter()
            .map(|(path, error)| format!("could not write {}: {error:#}", path.display())),
    );
    problems
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use pretty_assertions::assert_eq;
    use serial_test::serial;

    use super::*;

    fn appender_names(config: &Config) -> Vec<String> {
        let mut names: Vec<String> = config
            .appenders()
            .iter()
            .map(|appender| appender.name().to_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    #[serial]
    fn the_shipped_configuration_says_what_the_default_says() {
        clear_pending_files();
        let shipped = log4rs::config::load_config_file(CONFIG_FILE_NAME, deserializers())
            .expect("the shipped configuration is valid");
        let default = default_config();

        assert_eq!(appender_names(&shipped), appender_names(&default));
        assert_eq!(shipped.root().level(), default.root().level());
        assert_eq!(shipped.root().appenders(), default.root().appenders());
        clear_pending_files();
    }

    #[test]
    #[serial]
    fn a_broken_configuration_falls_back_to_the_default_and_reports_why() {
        let directory = std::env::temp_dir().join(format!("shinqlx-cfg-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path: PathBuf = directory.join(CONFIG_FILE_NAME);
        fs::write(&path, "appenders: [this is not a configuration").unwrap();

        let (config, error) = config_from_file_or_default(&path);

        assert!(error.is_some());
        assert_eq!(appender_names(&config), vec!["file", "stdout"]);
        clear_pending_files();
        let _ = fs::remove_dir_all(directory);
    }
}
