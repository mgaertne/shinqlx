//! Inspects recordings of shinqlx and exports demos from them.

mod cli;

use std::{io, process::ExitCode};

use clap::Parser;

fn main() -> ExitCode {
    match cli::Args::parse().run(&mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        // The reader of the output, like `head`, has seen enough.
        Err(error) if is_closed_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ql_demo: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn is_closed_pipe(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
}
