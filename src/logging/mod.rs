//! Logging of shinqlx: one log4rs configuration for the messages of Rust and Python.

#![allow(
    dead_code,
    unused_imports,
    reason = "the logging pieces are wired in step by step"
)]

mod async_appender;

pub(crate) use async_appender::AsyncAppender;
