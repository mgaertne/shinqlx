use std::path::PathBuf;

use clap::Parser;

/// Migrates a folder of minqlx plugins to shinqlx. Plugins that need no change are copied as they are.
#[derive(Debug, Parser)]
#[command(name = "plugin_migration", version, about)]
pub(crate) struct Args {
    /// Folder with the minqlx plugins.
    pub(crate) input: PathBuf,

    /// Folder the migrated plugins are written to. Must not exist or must be empty.
    pub(crate) output: PathBuf,

    /// Only report what would change, do not write anything.
    #[arg(short = 'n', long)]
    pub(crate) dry_run: bool,

    /// Number of worker threads, defaults to the number of CPUs.
    #[arg(short, long)]
    pub(crate) jobs: Option<usize>,
}
