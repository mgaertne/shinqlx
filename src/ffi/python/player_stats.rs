use crate::prelude::*;
use pyo3::prelude::*;

/// A player's score and some basic stats.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerStats", get_all)]
#[derive(Debug, PartialEq)]
pub(crate) struct PlayerStats {
    /// The player's primary score.
    pub(crate) score: i32,
    /// The player's number of kills.
    pub(crate) kills: i32,
    /// The player's number of deaths.
    pub(crate) deaths: i32,
    /// The player's total damage dealt.
    pub(crate) damage_dealt: i32,
    /// The player's total damage taken.
    pub(crate) damage_taken: i32,
    /// The time in milliseconds the player has on a team since the game started.
    pub(crate) time: i32,
    /// The player's ping.
    pub(crate) ping: i32,
}

#[pymethods]
impl PlayerStats {
    fn __str__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
                self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }

    fn __repr__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
                self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }
}

impl From<GameClient> for PlayerStats {
    fn from(game_client: GameClient) -> Self {
        Self {
            score: game_client.get_score(),
            kills: game_client.get_kills(),
            deaths: game_client.get_deaths(),
            damage_dealt: game_client.get_damage_dealt(),
            damage_taken: game_client.get_damage_taken(),
            time: game_client.get_time_on_team(),
            ping: game_client.get_ping(),
        }
    }
}
