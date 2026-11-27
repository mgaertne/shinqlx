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

    use super::*;

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
