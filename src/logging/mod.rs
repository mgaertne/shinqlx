//! Logging of shinqlx: one log4rs configuration for the messages of Rust and Python.

mod async_appender;
#[cfg(test)]
pub(crate) mod capture;
mod config;
mod dotted_encoder;
mod log_file;

#[allow(
    unused_imports,
    reason = "wired in with the Python logger in the next step"
)]
pub(crate) use async_appender::AsyncAppender;
pub(crate) use config::{finish_logging, initialize_logging};
pub(crate) use dotted_encoder::DottedPatternEncoder;
pub(crate) use log_file::RotationDefaults;
