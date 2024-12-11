use super::prelude::*;
use crate::ffi::c::prelude::*;

use core::fmt::{Display, Formatter};

/// A player's score and some basic stats.
#[pyclass(
    module = "_shinqlx",
    name = "PlayerStats",
    frozen,
    get_all,
    sequence,
    str
)]
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

impl Display for PlayerStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
            self.score,
            self.kills,
            self.deaths,
            self.damage_dealt,
            self.damage_taken,
            self.time,
            self.ping
        )
    }
}

#[pymethods]
impl PlayerStats {
    fn __repr__(&self) -> String {
        format!("{self}")
    }
}

#[cfg(test)]
mod player_stats_tests {
    use super::PlayerStats;
    use crate::ffi::c::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;

    fn default_player_stats() -> PlayerStats {
        PlayerStats {
            score: 42,
            kills: 7,
            deaths: 9,
            damage_dealt: 5000,
            damage_taken: 4200,
            time: 123,
            ping: 9,
        }
    }

    #[test]
    fn player_stats_to_str() {
        assert_eq!(
            format!("{}", default_player_stats()),
            "PlayerStats(score=42, kills=7, deaths=9, damage_dealt=5000, damage_taken=4200, time=123, ping=9)"
        );
    }

    #[test]
    fn player_stats_repr() {
        assert_eq!(
            default_player_stats().__repr__(),
            "PlayerStats(score=42, kills=7, deaths=9, damage_dealt=5000, damage_taken=4200, time=123, ping=9)"
        );
    }

    #[test]
    #[serial]
    fn player_state_from_game_client() {
        let mut mock_game_client = MockGameClient::new();
        mock_game_client.expect_get_score().returning(|| 42);
        mock_game_client.expect_get_kills().returning(|| 7);
        mock_game_client.expect_get_deaths().returning(|| 9);
        mock_game_client
            .expect_get_damage_dealt()
            .returning(|| 5000);
        mock_game_client
            .expect_get_damage_taken()
            .returning(|| 4200);
        mock_game_client.expect_get_time_on_team().returning(|| 123);
        mock_game_client.expect_get_ping().returning(|| 9);

        assert_eq!(PlayerStats::from(mock_game_client), default_player_stats())
    }
}
