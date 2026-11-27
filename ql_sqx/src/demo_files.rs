//! Writes the demos of a recording as files.

use alloc::{format, string::String, vec::Vec};
use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
};

use derive_more::Display;

use super::{
    archive::{ArchiveError, ArchiveReader},
    export::{ExportError, PrivateMessages, export_dm91},
    players::Stay,
    recorder::{RecordingError, RecordingStats},
};

const LONGEST_NAME_IN_FILE_NAMES: usize = 32;

/// Why no demo files were written.
#[derive(Debug, Display)]
pub enum DemoFileError {
    /// The recording cannot be opened.
    #[display("cannot open the recording: {_0}")]
    Open(io::Error),
    /// The recording cannot be read or a demo cannot be written.
    #[display("{_0}")]
    Export(ExportError),
    /// An unfinished demo could not be removed again.
    #[display("cannot remove the unfinished {}: {_1}", _0.display())]
    Cleanup(PathBuf, io::Error),
}

impl core::error::Error for DemoFileError {}

impl From<ArchiveError> for DemoFileError {
    fn from(error: ArchiveError) -> Self {
        Self::Export(error.into())
    }
}

impl From<ExportError> for DemoFileError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}

/// A demo that was written.
#[derive(Debug)]
pub struct DemoFile {
    /// The file.
    pub path: PathBuf,
    /// Whose view it shows.
    pub stay: Stay,
    /// What it contains.
    pub stats: RecordingStats,
}

/// What writing the demos of a recording did.
#[derive(Debug, Default)]
pub struct DemoFiles {
    /// The new demos, in the order of client and stay.
    pub written: Vec<DemoFile>,
    /// Files that existed already and were left alone.
    pub kept: Vec<PathBuf>,
}

/// The name of the demo of a stay: recording, client number, player and, from the second on, the
/// number of the stay.
pub fn demo_file_name(recording: &Path, stay: &Stay) -> String {
    let stem = recording.file_stem().unwrap_or_default().to_string_lossy();
    let player: String = stay
        .name
        .iter()
        .flat_map(|name| {
            core::iter::once('_').chain(name.chars().take(LONGEST_NAME_IN_FILE_NAMES).map(
                |character| match character {
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '-' => character,
                    _ => '_',
                },
            ))
        })
        .collect();
    let number = match stay.number {
        1 => String::new(),
        number => format!("_{number}"),
    };
    format!("{stem}_client{:02}{player}{number}.dm_91", stay.client)
}

/// Writes the demos of the selected stays into an existing directory without overwriting anything.
/// A failure leaves no new demos behind.
pub fn write_demo_files(
    recording: &Path,
    directory: &Path,
    private_messages: PrivateMessages,
    mut select: impl FnMut(&Stay) -> bool,
) -> Result<DemoFiles, DemoFileError> {
    let file = File::open(recording).map_err(DemoFileError::Open)?;
    let reader = ArchiveReader::new(BufReader::new(file))?;
    let mut created = Vec::new();
    let mut kept = Vec::new();
    let result = export_dm91(reader, private_messages, |stay| {
        if !select(stay) {
            return Ok(None);
        }
        let path = directory.join(demo_file_name(recording, stay));
        match File::create_new(&path) {
            Ok(file) => {
                created.push((stay.clone(), path));
                Ok(Some(BufWriter::new(file)))
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                kept.push(path);
                Ok(None)
            }
            Err(error) => Err(io::Error::new(
                error.kind(),
                format!("cannot create {}: {error}", path.display()),
            )),
        }
    });
    let demos = match result {
        Ok(demos) => demos,
        Err(error) => {
            for (_, path) in created {
                fs::remove_file(&path).map_err(|error| DemoFileError::Cleanup(path, error))?;
            }
            return Err(error.into());
        }
    };
    let written = demos
        .into_iter()
        .filter_map(|demo| {
            let (_, path) = created.iter().find(|(stay, _)| *stay == demo.stay)?;
            Some(DemoFile {
                path: path.clone(),
                stay: demo.stay,
                stats: demo.stats,
            })
        })
        .collect();
    Ok(DemoFiles { written, kept })
}

impl From<RecordingError> for DemoFileError {
    fn from(error: RecordingError) -> Self {
        Self::Export(error.into())
    }
}

#[cfg(test)]
mod demo_files_tests {
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;
    use crate::{archive::ArchiveWriter, capture::Frame, state::PlayerState, writer::Event};

    /// A folder with a recording of two players.
    fn recording() -> (TempDir, PathBuf) {
        let mut writer = ArchiveWriter::start(Vec::new(), &[], &[]).expect("started");
        writer
            .event(&Event::Frame(Frame {
                server_time: 25,
                entities: Vec::new(),
                players: vec![(0, PlayerState::default()), (1, PlayerState::default())],
            }))
            .expect("written");
        let folder = tempfile::tempdir().expect("a folder");
        let path = folder.path().join("match.dm_sqx");
        fs::write(&path, writer.finish().expect("finished").1).expect("stored");
        (folder, path)
    }

    fn write_all(recording: &Path, directory: &Path) -> Result<DemoFiles, DemoFileError> {
        write_demo_files(recording, directory, PrivateMessages::Include, |_| true)
    }

    #[test]
    #[cfg_attr(miri, ignore = "writes files")]
    fn the_selected_demos_are_written_once() {
        let (folder, recording) = recording();

        let first = write_demo_files(&recording, folder.path(), PrivateMessages::Omit, |stay| {
            stay.client == 1
        })
        .expect("written");
        let second = write_all(&recording, folder.path()).expect("written");

        assert_eq!(first.written.len(), 1);
        assert_eq!(first.written[0].stay.client, 1);
        assert_eq!(second.kept, [first.written[0].path.clone()]);
        assert_eq!(second.written.len(), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "reads files")]
    fn a_missing_recording_cannot_be_opened() {
        let folder = tempfile::tempdir().expect("a folder");

        let error =
            write_all(&folder.path().join("none.dm_sqx"), folder.path()).expect_err("missing");

        assert!(matches!(error, DemoFileError::Open(_)));
        assert!(error.to_string().starts_with("cannot open the recording: "));
    }

    #[test]
    #[cfg_attr(miri, ignore = "reads files")]
    fn a_demo_that_cannot_be_created_is_named() {
        let (folder, recording) = recording();

        let error = write_all(&recording, &folder.path().join("nowhere")).expect_err("no folder");

        assert!(error.to_string().contains("cannot create "), "{error}");
        assert!(error.to_string().contains("client00.dm_91"), "{error}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "writes files")]
    fn a_failure_leaves_no_new_demos_behind() {
        let (folder, recording) = recording();
        let mut content = fs::read(&recording).expect("readable");
        // A delta frame of one byte, which is damaged.
        content.extend_from_slice(&[4, 1, 0, 0, 0, 0xff]);
        fs::write(&recording, content).expect("stored");

        let error = write_all(&recording, folder.path()).expect_err("damaged");

        assert!(error.to_string().contains("damaged record"), "{error}");
        assert_eq!(fs::read_dir(folder.path()).expect("listed").count(), 1);
    }

    #[test]
    fn demos_are_named_after_recording_client_and_player() {
        let recording = Path::new("/demos/20260101_100000_aerowalk.dm_sqx");
        let stay = |client, number, name: Option<&str>| Stay {
            client,
            number,
            name: name.map(Into::into),
            steam_id: None,
        };

        assert_eq!(
            demo_file_name(recording, &stay(3, 1, Some("../a b"))),
            "20260101_100000_aerowalk_client03____a_b.dm_91"
        );
        assert_eq!(
            demo_file_name(recording, &stay(12, 2, Some("Me"))),
            "20260101_100000_aerowalk_client12_Me_2.dm_91"
        );
        assert_eq!(
            demo_file_name(recording, &stay(0, 1, None)),
            "20260101_100000_aerowalk_client00.dm_91"
        );
    }
}
