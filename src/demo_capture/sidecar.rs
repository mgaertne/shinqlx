//! What is known about a recording without reading it: a `.json` file next to it.

use alloc::{string::String, vec::Vec};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const EXTENSION: &str = "json";

/// The map and the start are required, so that other json files are not taken for sidecars.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub(crate) map: String,
    #[serde(default)]
    pub(crate) factory: String,
    /// Seconds since the Unix epoch, like all times here.
    pub(crate) started: u64,
    #[serde(default)]
    pub(crate) ended: Option<u64>,
    #[serde(default)]
    pub(crate) match_played: bool,
    #[serde(default)]
    pub(crate) players: Vec<PlayerEntry>,
    #[serde(default)]
    pub(crate) requests: Vec<Request>,
}

/// Someone who was in the game during the recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlayerEntry {
    /// Made up by the engine for bots.
    pub(crate) steam_id: u64,
    pub(crate) name: String,
    pub(crate) client_id: u8,
}

impl PlayerEntry {
    fn is_same_stay(&self, other: &Self) -> bool {
        self.client_id == other.client_id && self.steam_id == other.steam_id
    }
}

/// The wish to keep a recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Request {
    /// The SteamID of who asked, if a player did.
    #[serde(default)]
    pub(crate) requested_by: Option<u64>,
    #[serde(default)]
    pub(crate) reason: String,
    pub(crate) time: u64,
}

impl Sidecar {
    /// Adds new players and renames known ones, returning whether anything changed.
    pub(crate) fn add_players(&mut self, seen: impl IntoIterator<Item = PlayerEntry>) -> bool {
        seen.into_iter().fold(false, |changed, player| {
            match self
                .players
                .iter_mut()
                .find(|known| known.is_same_stay(&player))
            {
                Some(known) if *known == player => changed,
                Some(known) => {
                    *known = player;
                    true
                }
                None => {
                    self.players.push(player);
                    true
                }
            }
        })
    }

    /// Returns whether this was news.
    pub(crate) fn note_match(&mut self) -> bool {
        !core::mem::replace(&mut self.match_played, true)
    }

    pub(crate) fn is_requested(&self) -> bool {
        !self.requests.is_empty()
    }
}

pub(super) fn path_of(recording: &Path) -> PathBuf {
    recording.with_extension(EXTENSION)
}

/// `None` if the recording has no sidecar.
pub(super) fn read(recording: &Path) -> io::Result<Option<Sidecar>> {
    match fs::read(path_of(recording)) {
        Ok(content) => serde_json::from_slice(&content)
            .map(Some)
            .map_err(|error| invalid_data(&error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Replaces the sidecar as a whole, so that readers never see half of it.
pub(super) fn write(recording: &Path, sidecar: &Sidecar) -> io::Result<()> {
    let path = path_of(recording);
    let unfinished = path.with_extension("json.tmp");
    let content = serde_json::to_vec_pretty(sidecar).map_err(|error| invalid_data(&error))?;
    fs::write(&unfinished, content)?;
    fs::rename(unfinished, path)
}

// Without its `std` feature the error of serde_json does not convert into `io::Error`.
fn invalid_data(error: &serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod sidecar_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use tempfile::tempdir;

    use super::*;

    fn player(steam_id: u64, name: &str, client_id: u8) -> PlayerEntry {
        PlayerEntry {
            steam_id,
            name: name.into(),
            client_id,
        }
    }

    #[test]
    fn players_are_listed_once_per_stay_with_their_latest_name() {
        let mut sidecar = Sidecar::default();

        assert!(sidecar.add_players([player(7, "First", 0), player(8, "Second", 1)]));
        assert!(!sidecar.add_players([player(7, "First", 0), player(8, "Second", 1)]));
        assert!(sidecar.add_players([player(7, "Renamed", 0), player(9, "Third", 1)]));
        assert!(sidecar.add_players([player(7, "Renamed", 3)]));

        assert_eq!(
            sidecar.players,
            [
                player(7, "Renamed", 0),
                player(8, "Second", 1),
                player(9, "Third", 1),
                player(7, "Renamed", 3),
            ]
        );
    }

    #[test]
    fn a_match_is_noted_once() {
        let mut sidecar = Sidecar::default();

        assert!(sidecar.note_match());
        assert!(!sidecar.note_match());
        assert!(sidecar.match_played);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn what_is_written_next_to_a_recording_is_read_again() {
        let directory = tempdir().expect("a directory");
        let recording = directory.path().join("20260101_100000_aerowalk.dm_sqx");
        let sidecar = Sidecar {
            map: "aerowalk".into(),
            factory: "duel".into(),
            started: 10,
            ended: Some(20),
            match_played: true,
            players: vec![player(76_561_198_000_000_001, "^1Red", 2)],
            requests: vec![Request {
                requested_by: None,
                reason: "check".into(),
                time: 15,
            }],
        };

        assert_eq!(read(&recording).expect("readable"), None);
        write(&recording, &sidecar).expect("written");

        assert_eq!(read(&recording).expect("readable"), Some(sidecar));
        assert_eq!(fs::read_dir(directory.path()).expect("readable").count(), 1);
    }

    #[rstest]
    #[case::other_json("{\"name\": \"notes\"}")]
    #[case::broken("{")]
    #[cfg_attr(miri, ignore)]
    fn other_content_is_no_sidecar(#[case] content: &str) {
        let directory = tempdir().expect("a directory");
        let recording = directory.path().join("20260101_100000_aerowalk.dm_sqx");
        fs::write(path_of(&recording), content).expect("written");

        assert!(read(&recording).is_err());
    }
}
