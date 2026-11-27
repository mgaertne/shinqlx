use core::time::Duration;
use std::{
    fs::{self, File},
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use ql_sqx::{ArchiveReader, PrivateMessages, Stay, Summary, summarize, write_demo_files};

/// Inspects recordings (`.dm_sqx`) of shinqlx and converts them to demos.
#[derive(Debug, Parser)]
#[command(name = "ql_demo", version, about)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show what a recording contains and who played.
    Info {
        /// The recording.
        recording: PathBuf,
    },
    /// Write demos (`.dm_91`), by default one for every player. Existing files are kept.
    Export {
        /// The recordings.
        #[arg(required = true)]
        recordings: Vec<PathBuf>,
        #[command(flatten)]
        players: Players,
        /// The folder for the demos, next to each recording by default.
        #[arg(short, long)]
        directory: Option<PathBuf>,
        /// Leave out team chat and tells.
        #[arg(long)]
        no_private: bool,
    },
}

/// Whose demos to write; everyone without a choice.
#[derive(Debug, Default, clap::Args)]
struct Players {
    /// A client number as shown by `info`, may be repeated.
    #[arg(short, long = "client")]
    clients: Vec<u8>,
    /// A part of a player name, ignoring case and color codes, may be repeated.
    #[arg(short = 'p', long = "player")]
    names: Vec<String>,
}

impl Players {
    fn include(&self, stay: &Stay) -> bool {
        if self.clients.is_empty() && self.names.is_empty() {
            return true;
        }
        let name = stay.name.as_deref().unwrap_or_default().to_lowercase();
        self.clients.contains(&stay.client)
            || self
                .names
                .iter()
                .any(|part| name.contains(&part.to_lowercase()))
    }
}

/// What to write for every recording.
struct Conversion {
    players: Players,
    directory: Option<PathBuf>,
    private_messages: PrivateMessages,
}

impl Args {
    pub(crate) fn run(self, report: &mut impl Write) -> anyhow::Result<()> {
        match self.command {
            Command::Info { recording } => {
                let summary = summarize(open(&recording)?)?;
                Ok(write_summary(report, &summary)?)
            }
            Command::Export {
                recordings,
                players,
                directory,
                no_private,
            } => {
                let conversion = Conversion {
                    players,
                    directory,
                    private_messages: if no_private {
                        PrivateMessages::Omit
                    } else {
                        PrivateMessages::Include
                    },
                };
                export_each(report, &recordings, &conversion)
            }
        }
    }
}

fn open(path: &Path) -> anyhow::Result<ArchiveReader<BufReader<File>>> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    ArchiveReader::new(BufReader::new(file))
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Minutes and seconds.
fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn write_summary(report: &mut impl Write, summary: &Summary) -> io::Result<()> {
    for (key, value) in &summary.meta {
        writeln!(report, "{key}: {value}")?;
    }
    writeln!(
        report,
        "length: {} ({} frames)",
        clock(summary.length),
        summary.frames
    )?;
    match &summary.end {
        Some(reason) => writeln!(report, "end: {reason}")?,
        None => writeln!(report, "end: incomplete recording")?,
    }
    if summary.truncated {
        writeln!(report, "the file ends in the middle of a record")?;
    }
    writeln!(report, "client    from   until  name")?;
    for entry in &summary.stays {
        writeln!(
            report,
            "{:>6}  {:>6}  {:>6}  {}",
            entry.stay.client,
            clock(entry.from),
            clock(entry.until),
            entry.stay.name.as_deref().unwrap_or_default()
        )?;
    }
    Ok(())
}

/// Converts all recordings, even if some of them fail.
fn export_each(
    report: &mut impl Write,
    recordings: &[PathBuf],
    conversion: &Conversion,
) -> anyhow::Result<()> {
    if let Some(directory) = &conversion.directory {
        fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    let mut failures = 0;
    for recording in recordings {
        match export(recording, conversion) {
            Ok(lines) => {
                for line in lines {
                    writeln!(report, "{line}")?;
                }
            }
            Err(error) => {
                eprintln!("ql_demo: {}: {error:#}", recording.display());
                failures += 1;
            }
        }
    }
    if failures > 0 {
        bail!(
            "{failures} of {} recordings could not be converted",
            recordings.len()
        );
    }
    Ok(())
}

/// Converts one recording, returning what to report about each demo.
fn export(recording: &Path, conversion: &Conversion) -> anyhow::Result<Vec<String>> {
    let directory = conversion
        .directory
        .as_deref()
        .or_else(|| recording.parent())
        .unwrap_or(Path::new(""));
    let demos = write_demo_files(recording, directory, conversion.private_messages, |stay| {
        conversion.players.include(stay)
    })?;
    if demos.written.is_empty() && demos.kept.is_empty() {
        bail!("no player of the recording matches");
    }
    let kept = demos
        .kept
        .iter()
        .map(|path| format!("{}: exists already, kept", path.display()));
    let written = demos.written.iter().map(|demo| {
        format!(
            "{}: {} snapshots, {} commands",
            demo.path.display(),
            demo.stats.snapshots,
            demo.stats.commands
        )
    });
    Ok(kept.chain(written).collect())
}

#[cfg(test)]
mod cli_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    fn stay(client: u8, number: u32, name: &str) -> Stay {
        Stay {
            client,
            number,
            name: Some(name.into()),
            steam_id: None,
        }
    }

    #[rstest]
    #[case::everyone(&[], &[], true)]
    #[case::by_number(&[3], &[], true)]
    #[case::another_number(&[4], &[], false)]
    #[case::by_part_of_the_name(&[], &["ONE"], true)]
    #[case::another_name(&[], &["two"], false)]
    #[case::either(&[4], &["some"], true)]
    fn players_are_chosen_by_number_or_name(
        #[case] clients: &[u8],
        #[case] names: &[&str],
        #[case] included: bool,
    ) {
        let players = Players {
            clients: clients.to_vec(),
            names: names.iter().map(|&name| name.into()).collect(),
        };

        assert_eq!(players.include(&stay(3, 1, "Someone")), included);
    }

    #[test]
    fn the_options_of_export_have_short_and_long_names() {
        for arguments in [
            "ql_demo export a b -c 1 -p x -d out --no-private",
            "ql_demo export a b --client 1 --player x --directory out --no-private",
        ] {
            let parsed = Args::try_parse_from(arguments.split(' '));

            assert!(
                matches!(
                    &parsed,
                    Ok(Args { command: Command::Export { recordings, players, directory, no_private: true } })
                        if recordings.len() == 2
                            && players.clients == [1]
                            && players.names == ["x"]
                            && directory.is_some()
                ),
                "{parsed:?}"
            );
        }
    }
}
