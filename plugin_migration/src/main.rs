//! Migrates a folder of minqlx plugins to shinqlx.

mod cli;
mod edit;
mod migration;
mod report;
mod rules;
mod run;
mod source;
mod stub;
mod tree;
mod walk;

use clap::Parser;
use cli::Args;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let reports = run::run(&args.input, &args.output, args.dry_run, args.jobs)?;
    print!("{}", report::render(&reports, args.dry_run));
    Ok(())
}
