//! What happens in a recording, derived from its frames and configstrings.

use alloc::{collections::BTreeMap, vec::Vec};
use core::mem::discriminant;

use super::{
    archive::Record,
    capture::Frame,
    state::EntityState,
    view::{EntityKind, MeansOfDeath, Weapon},
    writer::Event,
};

const WARMUP: u16 = 5;
const FIRST_SCORE: u16 = 6;
const SECOND_SCORE: u16 = 7;
const ROUND_STATUS: u16 = 661;
const ROUND_TIME: u16 = 662;
const INTERMISSION: u16 = 14;
const FIRST_PLAYER: u16 = 529;
const CLIENTS: u16 = 64;
/// The score of a place nobody holds.
const NO_SCORE: i32 = -9999;
const EVENT_FIRE_WEAPON: u32 = 20;
const EVENT_OBITUARY: u32 = 58;

/// Something that happened, and the server time of the frame that shows it first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameEvent {
    /// The server time in milliseconds.
    pub time: i32,
    /// What happened.
    pub happening: Happening,
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Happening {
    /// A player died.
    Kill {
        /// The client number of who died.
        victim: u8,
        /// The client number of who is responsible, `None` for the map.
        killer: Option<u8>,
        /// How, `None` for a way that is not known.
        means: Option<MeansOfDeath>,
    },
    /// A player fired a weapon; automatic weapons do so for every shot.
    Shot {
        /// The client number.
        client: u8,
        /// The weapon.
        weapon: Weapon,
    },
    /// The warmup changed. The start of a recording tells it as it was.
    Warmup(Warmup),
    /// The next round of a round based game is announced.
    RoundCountdown {
        /// The number of the round, starting with 1.
        number: u32,
        /// The server time it starts at.
        starts: i32,
    },
    /// A round started.
    RoundStart {
        /// The number of the round.
        number: u32,
    },
    /// The running round was decided.
    RoundEnd {
        /// The number of the round.
        number: u32,
    },
    /// The scores changed: of red and blue in team games, of the first two places otherwise.
    Scores {
        /// Red or the leader, `None` if nobody holds the place.
        first: Option<i32>,
        /// Blue or the runner-up, `None` if nobody holds the place.
        second: Option<i32>,
    },
    /// A player joined a team or left the server. The start of a recording tells all teams.
    Team {
        /// The client number.
        client: u8,
        /// The team, `None` if the player is gone.
        team: Option<Team>,
    },
}

/// Whether the match runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warmup {
    /// The game waits for players to be ready.
    Waiting,
    /// The match starts at this server time.
    Countdown {
        /// The server time in milliseconds.
        starts: i32,
    },
    /// The match runs or has just ended.
    Over,
}

/// A team of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    /// The team of games without teams.
    Free,
    /// The red team.
    Red,
    /// The blue team.
    Blue,
    /// Those who watch.
    Spectator,
}

/// Follows a recording record by record and tells what happens in it.
#[derive(Debug, Default)]
pub struct Timeline {
    time: i32,
    /// What configstrings told since the last frame.
    pending: Vec<Happening>,
    scores: [Option<i32>; 2],
    /// The number of the round that runs.
    round: Option<u32>,
    teams: BTreeMap<u8, Team>,
    /// The event entities of the last frame, which stay for a while.
    event_entities: Vec<EntityState>,
    /// The number of the next event of every player.
    sequences: BTreeMap<u8, u32>,
}

impl Timeline {
    /// What the next record of the recording adds, in the order it happened.
    pub fn record(&mut self, record: &Record) -> Vec<GameEvent> {
        match record {
            Record::Start { configstrings, .. } => {
                for (index, value) in configstrings {
                    self.configstring(*index, value);
                }
            }
            Record::Event(Event::Configstring { index, value }) => {
                self.configstring(*index, value);
            }
            Record::Event(Event::PlayerLeft { client }) => {
                self.sequences.remove(client);
            }
            Record::Event(Event::Frame(frame)) => return self.frame(frame),
            Record::Event(Event::End { .. }) => {
                let pending = core::mem::take(&mut self.pending);
                return self.stamped(pending);
            }
            Record::Event(Event::Command { .. } | Event::Stats { .. }) => {}
        }
        Vec::new()
    }

    fn frame(&mut self, frame: &Frame) -> Vec<GameEvent> {
        self.time = frame.server_time;
        let mut happened = core::mem::take(&mut self.pending);
        self.shots(frame, &mut happened);
        self.kills(frame, &mut happened);
        self.stamped(happened)
    }

    fn stamped(&self, happened: Vec<Happening>) -> Vec<GameEvent> {
        happened
            .into_iter()
            .map(|happening| GameEvent {
                time: self.time,
                happening,
            })
            .collect()
    }

    fn shots(&mut self, frame: &Frame, happened: &mut Vec<Happening>) {
        for (client, player) in &frame.players {
            // A spectator carries the state of whom they follow.
            if player.client_number() != i32::from(*client) {
                self.sequences.remove(client);
                continue;
            }
            let next = player.event_sequence();
            let Some(before) = self.sequences.insert(*client, next) else {
                continue;
            };
            let Some(weapon) = player.weapon() else {
                continue;
            };
            happened.extend(
                player
                    .events_since(before)
                    .filter(|&event| event == EVENT_FIRE_WEAPON)
                    .map(|_| Happening::Shot {
                        client: *client,
                        weapon,
                    }),
            );
        }
    }

    fn kills(&mut self, frame: &Frame, happened: &mut Vec<Happening>) {
        let event_entities: Vec<EntityState> = frame
            .entities
            .iter()
            .map(|entity| entity.state)
            .filter(|state| matches!(state.kind(), EntityKind::Event(_)))
            .collect();
        happened.extend(
            event_entities
                .iter()
                .filter(|state| state.kind() == EntityKind::Event(EVENT_OBITUARY))
                .filter(|state| !self.event_entities.iter().any(|old| same_event(old, state)))
                .filter_map(|state| {
                    Some(Happening::Kill {
                        victim: client(state.other_entity())?,
                        killer: client(state.second_other_entity()),
                        means: MeansOfDeath::from_number(state.event_parameter()),
                    })
                }),
        );
        self.event_entities = event_entities;
    }

    fn configstring(&mut self, index: u16, value: &[u8]) {
        let value = core::str::from_utf8(value).unwrap_or_default();
        match index {
            WARMUP => {
                let warmup =
                    info_value(value, "time").and_then(|time| match time.parse().ok()? {
                        -1 => Some(Warmup::Waiting),
                        0 => Some(Warmup::Over),
                        starts => Some(Warmup::Countdown { starts }),
                    });
                if let Some(warmup) = warmup {
                    self.replace(Happening::Warmup(warmup));
                }
            }
            ROUND_STATUS => self.round_status(value),
            ROUND_TIME if value == "-1" => self.end_round(),
            // The game does not report the end of the round that decides a match.
            INTERMISSION if value == "1" => self.end_round(),
            FIRST_SCORE | SECOND_SCORE => self.score(usize::from(index - FIRST_SCORE), value),
            _ => {
                if let Some(client) = index
                    .checked_sub(FIRST_PLAYER)
                    .filter(|&client| client < CLIENTS)
                    .and_then(|client| u8::try_from(client).ok())
                {
                    self.team(client, value);
                }
            }
        }
    }

    fn score(&mut self, place: usize, value: &str) {
        let score = value.parse().ok().filter(|&score| score != NO_SCORE);
        if self.scores[place] == score {
            return;
        }
        self.scores[place] = score;
        let [first, second] = self.scores;
        self.replace(Happening::Scores { first, second });
    }

    /// Keeps only the latest of the happenings the game passes through within one frame.
    fn replace(&mut self, happening: Happening) {
        self.pending
            .retain(|pending| discriminant(pending) != discriminant(&happening));
        self.pending.push(happening);
    }

    fn end_round(&mut self) {
        let ended = self
            .round
            .take()
            .map(|number| Happening::RoundEnd { number });
        self.pending.extend(ended);
    }

    fn round_status(&mut self, value: &str) {
        let Some(number) = info_value(value, "round")
            .and_then(|number| number.parse().ok())
            .filter(|&number| number > 0)
        else {
            return;
        };
        match info_value(value, "time").and_then(|time| time.parse().ok()) {
            Some(starts) => self
                .pending
                .push(Happening::RoundCountdown { number, starts }),
            None if self.round != Some(number) => {
                self.round = Some(number);
                self.pending.push(Happening::RoundStart { number });
            }
            None => {}
        }
    }

    fn team(&mut self, client: u8, value: &str) {
        let team = info_value(value, "t").and_then(|team| match team {
            "0" => Some(Team::Free),
            "1" => Some(Team::Red),
            "2" => Some(Team::Blue),
            "3" => Some(Team::Spectator),
            _ => None,
        });
        let before = match team {
            Some(team) => self.teams.insert(client, team),
            None => self.teams.remove(&client),
        };
        if before != team {
            self.pending.push(Happening::Team { client, team });
        }
    }
}

fn same_event(old: &EntityState, new: &EntityState) -> bool {
    old.number == new.number
        && old.kind() == new.kind()
        && old.other_entity() == new.other_entity()
        && old.second_other_entity() == new.second_other_entity()
        && old.event_parameter() == new.event_parameter()
}

fn client(number: u32) -> Option<u8> {
    u8::try_from(number)
        .ok()
        .filter(|&number| u16::from(number) < CLIENTS)
}

/// The value of a key in a string like `\key\value\key\value`, with or without the first backslash.
fn info_value<'a>(info: &'a str, key: &str) -> Option<&'a str> {
    let mut parts = info.strip_prefix('\\').unwrap_or(info).split('\\');
    while let Some(name) = parts.next() {
        let value = parts.next()?;
        if name == key {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod events_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        capture::{CapturedEntity, Visibility},
        fields::{entity_field, player_field},
        state::PlayerState,
    };

    fn obituary(number: u16, victim: u32, killer: u32) -> CapturedEntity {
        let mut state = EntityState {
            number,
            ..EntityState::default()
        };
        state.fields[entity_field("eType")] = 13 + EVENT_OBITUARY;
        state.fields[entity_field("otherEntityNum")] = victim;
        state.fields[entity_field("otherEntityNum2")] = killer;
        state.fields[entity_field("eventParm")] = 10;
        CapturedEntity {
            state,
            visibility: Visibility::Everyone,
        }
    }

    /// The state of client 2, whose events so far are `events`, the newest last.
    fn player(sequence: u32, events: [u32; 2]) -> PlayerState {
        let mut player = PlayerState::default();
        let mut set = |name, value| player.fields[player_field(name)] = value;
        set("clientNum", 2);
        set("weapon", 7);
        set("eventSequence", sequence);
        set("events[0]", events[0]);
        set("events[1]", events[1]);
        player
    }

    fn frame(
        server_time: i32,
        entities: Vec<CapturedEntity>,
        players: Vec<(u8, PlayerState)>,
    ) -> Record {
        Record::Event(Event::Frame(Frame {
            server_time,
            entities,
            players,
        }))
    }

    fn configstring(index: u16, value: &str) -> Record {
        Record::Event(Event::Configstring {
            index,
            value: value.into(),
        })
    }

    fn happenings(timeline: &mut Timeline, record: &Record) -> Vec<Happening> {
        timeline
            .record(record)
            .into_iter()
            .map(|event| event.happening)
            .collect()
    }

    #[test]
    fn a_death_is_told_once_while_its_entity_stays() {
        let mut timeline = Timeline::default();
        let killed = || vec![obituary(100, 3, 2), obituary(101, 4, 1022)];

        let first = timeline.record(&frame(25, killed(), Vec::new()));
        let again = timeline.record(&frame(50, killed(), Vec::new()));

        let kill = |victim, killer| GameEvent {
            time: 25,
            happening: Happening::Kill {
                victim,
                killer,
                means: Some(MeansOfDeath::Railgun),
            },
        };
        assert_eq!(first, [kill(3, Some(2)), kill(4, None)]);
        assert_eq!(again, []);
    }

    #[test]
    fn every_new_event_of_a_player_that_fires_is_a_shot() {
        let mut timeline = Timeline::default();
        let mut shots = |sequence, events| {
            happenings(
                &mut timeline,
                &frame(25, Vec::new(), vec![(2, player(sequence, events))]),
            )
            .len()
        };

        assert_eq!(shots(4, [0, 20]), 0);
        assert_eq!(shots(5, [20, 20]), 1);
        assert_eq!(shots(5, [20, 20]), 0);
        assert_eq!(shots(7, [20, 20]), 2);
        assert_eq!(shots(8, [20, 1]), 0);
        assert_eq!(shots(65535, [20, 1]), 1);
        assert_eq!(shots(0, [20, 20]), 1);
    }

    #[test]
    fn the_followed_player_does_not_shoot_twice() {
        let mut timeline = Timeline::default();
        for sequence in [1, 2] {
            let spectator = vec![(5, player(sequence, [20, 20]))];

            let told = timeline.record(&frame(25, Vec::new(), spectator));

            assert_eq!(told, []);
        }
    }

    #[test]
    fn rounds_are_announced_started_and_ended() {
        let mut timeline = Timeline::default();
        let mut told = |index, value| {
            timeline.record(&configstring(index, value));
            happenings(&mut timeline, &frame(25, Vec::new(), Vec::new()))
        };

        assert_eq!(
            told(ROUND_STATUS, "\\time\\9000\\round\\2"),
            [Happening::RoundCountdown {
                number: 2,
                starts: 9000
            }]
        );
        assert_eq!(told(ROUND_TIME, "9000"), []);
        assert_eq!(
            told(ROUND_STATUS, "\\round\\2"),
            [Happening::RoundStart { number: 2 }]
        );
        assert_eq!(told(ROUND_STATUS, "\\round\\2"), []);
        assert_eq!(told(ROUND_TIME, "-1"), [Happening::RoundEnd { number: 2 }]);
        assert_eq!(told(ROUND_TIME, "-1"), []);
    }

    #[test]
    fn the_intermission_ends_the_deciding_round() {
        let mut timeline = Timeline::default();
        for record in [
            configstring(ROUND_STATUS, "\\time\\-1\\round\\0"),
            configstring(ROUND_STATUS, "\\round\\5"),
            configstring(INTERMISSION, "1"),
        ] {
            timeline.record(&record);
        }

        let told = happenings(&mut timeline, &frame(25, Vec::new(), Vec::new()));

        assert_eq!(
            told,
            [
                Happening::RoundStart { number: 5 },
                Happening::RoundEnd { number: 5 }
            ]
        );
    }

    #[test]
    fn only_the_latest_warmup_of_a_frame_is_told() {
        let mut timeline = Timeline::default();
        timeline.record(&configstring(WARMUP, "\\time\\-1"));
        timeline.record(&configstring(WARMUP, "\\time\\0"));

        let told = happenings(&mut timeline, &frame(25, Vec::new(), Vec::new()));

        assert_eq!(told, [Happening::Warmup(Warmup::Over)]);
    }

    #[test]
    fn configstrings_tell_warmup_scores_and_teams_with_the_next_frame() {
        let mut timeline = Timeline::default();
        for record in [
            configstring(WARMUP, "\\time\\5000"),
            configstring(FIRST_SCORE, "3"),
            configstring(SECOND_SCORE, "-9999"),
            configstring(FIRST_PLAYER + 2, "n\\Someone\\t\\1"),
            configstring(FIRST_PLAYER + 2, "n\\Renamed\\t\\1"),
            configstring(FIRST_PLAYER + 2, ""),
        ] {
            assert_eq!(timeline.record(&record), []);
        }

        let told = happenings(&mut timeline, &frame(25, Vec::new(), Vec::new()));

        assert_eq!(
            told,
            [
                Happening::Warmup(Warmup::Countdown { starts: 5000 }),
                Happening::Scores {
                    first: Some(3),
                    second: None
                },
                Happening::Team {
                    client: 2,
                    team: Some(Team::Red)
                },
                Happening::Team {
                    client: 2,
                    team: None
                },
            ]
        );
    }
}
