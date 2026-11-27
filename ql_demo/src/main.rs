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

#[cfg(test)]
mod main_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(io::ErrorKind::BrokenPipe.into(), true)]
    #[case(io::ErrorKind::NotFound.into(), false)]
    fn only_a_closed_pipe_is_no_failure(#[case] error: io::Error, #[case] closed: bool) {
        assert_eq!(is_closed_pipe(&error.into()), closed);
    }
}
