//! What is known about a recording without reading it: a `.json` file next to it.

use alloc::{string::String, vec::Vec};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const EXTENSION: &str = "json";

/// The map and the start are required, so that other json files are not taken for sidecars.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub(crate) map: String,
    #[serde(default)]
    pub(crate) factory: String,
    /// The short name like `ca`.
    #[serde(default)]
    pub(crate) gametype: String,
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
    /// The identifier the stats of the game give the match.
    #[serde(default)]
    pub(crate) match_guid: Option<String>,
    #[serde(default)]
    pub(crate) result: Option<MatchResult>,
    /// The latest standing of everyone the game reported on.
    #[serde(default)]
    pub(crate) scores: Vec<PlayerScore>,
}

/// How the match ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MatchResult {
    pub(crate) aborted: bool,
    pub(crate) red_score: i32,
    pub(crate) blue_score: i32,
}

/// What the game reported about a player at the end of the match or when leaving it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlayerScore {
    pub(crate) steam_id: u64,
    pub(crate) name: String,
    /// `free`, `red`, `blue` or `spectator`.
    pub(crate) team: String,
    pub(crate) score: i32,
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

    /// Takes the match, its result and the scores from a stats message of the game, returning
    /// whether anything changed. What happens in the warmup is not part of the match.
    pub(crate) fn note_stats(&mut self, message: &str) -> bool {
        let Ok(message) = serde_json::from_str::<Value>(message) else {
            return false;
        };
        let (kind, data) = (message["TYPE"].as_str(), &message["DATA"]);
        if data["WARMUP"].as_bool() == Some(true) {
            return false;
        }
        let Some(guid) = data["MATCH_GUID"].as_str().filter(|guid| !guid.is_empty()) else {
            return false;
        };
        let mut changed = false;
        if kind == Some("MATCH_STARTED") || self.match_guid.is_none() {
            changed = self.begin_match(guid);
        }
        if self.match_guid.as_deref() != Some(guid) {
            return changed;
        }
        match kind {
            Some("MATCH_REPORT") => self.note_result(data) || changed,
            Some("PLAYER_STATS") => self.note_score(data) || changed,
            _ => changed,
        }
    }

    fn begin_match(&mut self, guid: &str) -> bool {
        if self.match_guid.as_deref() == Some(guid) {
            return false;
        }
        self.match_guid = Some(guid.into());
        self.result = None;
        self.scores.clear();
        true
    }

    fn note_result(&mut self, data: &Value) -> bool {
        let result = MatchResult {
            aborted: data["ABORTED"].as_bool().unwrap_or_default(),
            red_score: number(&data["TSCORE0"]),
            blue_score: number(&data["TSCORE1"]),
        };
        self.result.replace(result) != Some(result)
    }

    fn note_score(&mut self, data: &Value) -> bool {
        let score = PlayerScore {
            steam_id: data["STEAM_ID"]
                .as_str()
                .and_then(|steam_id| steam_id.parse().ok())
                .unwrap_or_default(),
            name: data["NAME"].as_str().unwrap_or_default().into(),
            team: team_name(number(&data["TEAM"])).into(),
            score: number(&data["SCORE"]),
        };
        match self
            .scores
            .iter_mut()
            .find(|known| known.steam_id == score.steam_id && known.name == score.name)
        {
            Some(known) if *known == score => false,
            Some(known) => {
                *known = score;
                true
            }
            None => {
                self.scores.push(score);
                true
            }
        }
    }
}

fn number(value: &Value) -> i32 {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .unwrap_or_default()
}

fn team_name(team: i32) -> &'static str {
    match team {
        1 => "red",
        2 => "blue",
        3 => "spectator",
        _ => "free",
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
            gametype: "duel".into(),
            match_guid: Some("guid".into()),
            result: Some(MatchResult {
                aborted: false,
                red_score: 3,
                blue_score: 1,
            }),
            scores: vec![PlayerScore {
                steam_id: 76_561_198_000_000_001,
                name: "^1Red".into(),
                team: "red".into(),
                score: 12,
            }],
        };

        assert_eq!(read(&recording).expect("readable"), None);
        write(&recording, &sidecar).expect("written");

        assert_eq!(read(&recording).expect("readable"), Some(sidecar));
        assert_eq!(fs::read_dir(directory.path()).expect("readable").count(), 1);
    }

    fn stats(kind: &str, data: &str) -> String {
        format!(r#"{{"DATA":{{{data}}},"TYPE":"{kind}"}}"#)
    }

    #[test]
    fn the_stats_of_a_match_describe_it() {
        let mut sidecar = Sidecar::default();
        let messages = [
            stats("MATCH_STARTED", r#""MATCH_GUID":"first""#),
            stats(
                "MATCH_REPORT",
                r#""MATCH_GUID":"first","ABORTED":false,"TSCORE0":3,"TSCORE1":1"#,
            ),
            stats(
                "PLAYER_STATS",
                r#""MATCH_GUID":"first","WARMUP":false,"STEAM_ID":"7","NAME":"Red","TEAM":1,"SCORE":4"#,
            ),
            stats(
                "PLAYER_STATS",
                r#""MATCH_GUID":"first","WARMUP":false,"STEAM_ID":"7","NAME":"Red","TEAM":1,"SCORE":9"#,
            ),
        ];

        assert!(messages.iter().all(|message| sidecar.note_stats(message)));
        assert!(!sidecar.note_stats(&messages[3]));

        assert_eq!(sidecar.match_guid.as_deref(), Some("first"));
        assert_eq!(
            sidecar.result,
            Some(MatchResult {
                aborted: false,
                red_score: 3,
                blue_score: 1,
            })
        );
        assert_eq!(
            sidecar.scores,
            [PlayerScore {
                steam_id: 7,
                name: "Red".into(),
                team: "red".into(),
                score: 9,
            }]
        );
    }

    #[test]
    fn the_next_match_replaces_the_one_before() {
        let mut sidecar = Sidecar::default();
        sidecar.note_stats(&stats(
            "MATCH_REPORT",
            r#""MATCH_GUID":"first","ABORTED":true"#,
        ));

        assert!(sidecar.note_stats(&stats("MATCH_STARTED", r#""MATCH_GUID":"second""#)));

        assert_eq!(sidecar.match_guid.as_deref(), Some("second"));
        assert_eq!(sidecar.result, None);
    }

    #[rstest]
    #[case::warmup(stats("PLAYER_STATS", r#""MATCH_GUID":"other","WARMUP":true"#))]
    #[case::without_guid(stats("ROUND_OVER", r#""ROUND":1"#))]
    #[case::broken("{".into())]
    fn other_stats_change_nothing(#[case] message: String) {
        let mut sidecar = Sidecar::default();

        assert!(!sidecar.note_stats(&message));
        assert_eq!(sidecar, Sidecar::default());
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
