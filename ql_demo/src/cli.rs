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
    use ql_sqx::{ArchiveWriter, Event, Frame, PlayerState};
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    const NAME: &str = "20260101_100000_aerowalk";

    /// A recording of two seconds with one player.
    fn recording(end: Option<&str>) -> (TempDir, PathBuf) {
        let frame = |server_time| {
            Event::Frame(Frame {
                server_time,
                entities: Vec::new(),
                players: vec![(3, PlayerState::default())],
            })
        };
        let meta = [("map".to_owned(), "aerowalk".to_owned())];
        let name = [(529 + 3, b"n\\^1Someone".to_vec())];
        let mut writer = ArchiveWriter::start(Vec::new(), &meta, &name).expect("started");
        let end = end.map(|reason| Event::End {
            reason: reason.into(),
        });
        for event in [frame(1000), frame(63_000)].into_iter().chain(end) {
            writer.event(&event).expect("written");
        }
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join(format!("{NAME}.dm_sqx"));
        fs::write(&path, writer.finish().expect("finished").1).expect("stored");
        (directory, path)
    }

    fn run(arguments: &[&str]) -> (anyhow::Result<()>, String) {
        let arguments = core::iter::once("ql_demo").chain(arguments.iter().copied());
        let mut report = Vec::new();
        let result = Args::try_parse_from(arguments)
            .expect("valid arguments")
            .run(&mut report);
        (result, String::from_utf8(report).expect("text"))
    }

    fn text(path: &Path) -> &str {
        path.to_str().expect("utf-8")
    }

    #[test]
    fn info_shows_meta_data_length_end_and_players() {
        let (_directory, path) = recording(Some("the match ended"));

        let (result, report) = run(&["info", text(&path)]);

        result.expect("readable");
        assert_eq!(
            report,
            "map: aerowalk\n\
             length: 1:02 (2 frames)\n\
             end: the match ended\n\
             client    from   until  name\n     \
             3    0:00    1:02  Someone\n"
        );
    }

    #[test]
    fn info_tells_about_a_recording_that_was_cut_off() {
        let (_directory, path) = recording(None);
        let content = fs::read(&path).expect("readable");
        fs::write(&path, &content[..content.len() - 1]).expect("stored");

        let (result, report) = run(&["info", text(&path)]);

        result.expect("readable");
        assert!(report.contains("end: incomplete recording\n"), "{report}");
        assert!(
            report.contains("the file ends in the middle of a record\n"),
            "{report}"
        );
    }

    #[rstest]
    #[case::missing(None, "cannot open")]
    #[case::something_else(Some("text"), "cannot read")]
    fn info_names_the_file_it_cannot_use(#[case] content: Option<&str>, #[case] problem: &str) {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("broken.dm_sqx");
        if let Some(content) = content {
            fs::write(&path, content).expect("stored");
        }

        let (result, _) = run(&["info", text(&path)]);

        let message = format!("{:#}", result.expect_err("unusable"));
        assert!(
            message.starts_with(problem) && message.contains("broken.dm_sqx"),
            "{message}"
        );
    }

    #[test]
    fn export_writes_demos_next_to_the_recording_and_keeps_existing_ones() {
        let (directory, path) = recording(None);
        let demo = directory
            .path()
            .join(format!("{NAME}_client03_Someone.dm_91"));

        let (first, written) = run(&["export", "--no-private", text(&path)]);
        let (second, kept) = run(&["export", text(&path)]);

        first.expect("exported");
        second.expect("nothing to do");
        assert_eq!(
            written,
            format!("{}: 2 snapshots, 0 commands\n", demo.display())
        );
        assert_eq!(kept, format!("{}: exists already, kept\n", demo.display()));
    }

    #[test]
    fn export_creates_the_folder_and_converts_the_others_if_one_fails() {
        let (directory, path) = recording(None);
        let demos = directory.path().join("some/demos");
        let missing = directory.path().join("missing.dm_sqx");

        let (result, report) = run(&["export", "-d", text(&demos), text(&missing), text(&path)]);

        assert_eq!(
            result.expect_err("one failed").to_string(),
            "1 of 2 recordings could not be converted"
        );
        assert!(report.contains("some/demos/"), "{report}");
    }

    #[test]
    fn export_fails_if_no_player_matches() {
        let (_directory, path) = recording(None);

        let (result, report) = run(&["export", "-c", "4", text(&path)]);

        assert!(result.is_err());
        assert_eq!(report, "");
    }

    #[test]
    fn export_names_the_folder_it_cannot_create() {
        let (_directory, path) = recording(None);

        let (result, _) = run(&["export", "-d", text(&path), text(&path)]);

        let message = format!("{:#}", result.expect_err("a file is in the way"));
        assert!(message.starts_with("cannot create"), "{message}");
    }

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
