//! When a recording starts and ends: one file holds the warmup and the match that follows it.

use core::time::Duration;
use std::time::Instant;

use derive_more::Display;

/// After a recording failed, the next one is not tried before this has passed.
const RETRY_DELAY: Duration = Duration::from_secs(60);

/// The state of the game, as the cvar `g_gameState` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    /// Players warm up.
    Warmup,
    /// The match is about to start.
    Countdown,
    /// The match runs, including what follows it until the game starts over.
    InProgress,
}

impl GameState {
    /// The state named by the cvar, `None` for unknown names.
    pub fn from_cvar(value: &str) -> Option<Self> {
        match value {
            "PRE_GAME" => Some(Self::Warmup),
            "COUNT_DOWN" => Some(Self::Countdown),
            "IN_PROGRESS" => Some(Self::InProgress),
            _ => None,
        }
    }
}

/// When the server shall record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rules {
    /// Whether to record at all.
    pub enabled: bool,
    /// Whether to record only while a human is connected.
    pub require_human: bool,
    /// After this a recording is continued in a new file.
    pub max_length: Option<Duration>,
}

/// What the server looks like at one moment.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    /// The moment.
    pub now: Instant,
    /// The state of the game, if known.
    pub game_state: Option<GameState>,
    /// Whether anyone is connected, bots included.
    pub player_connected: bool,
    /// Whether someone other than a bot is connected.
    pub human_connected: bool,
    /// Whether there is disk space left for recordings.
    pub has_room: bool,
}

/// Why a recording ends.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// Recording was switched off.
    #[display("recording was disabled")]
    Disabled,
    /// The server is empty, or the last human left where one is required.
    #[display("no player is left")]
    NoPlayer,
    /// The game started over after a match, finished or aborted.
    #[display("the match ended")]
    MatchEnded,
    /// The file reached its maximum length.
    #[display("the maximum length was reached")]
    MaxLength,
    /// There is no space left for recordings.
    #[display("the disk is full")]
    DiskFull,
}

/// What to do about the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Start a recording.
    Start,
    /// End the running recording.
    Finish(EndReason),
    /// End the running recording and start the next one.
    Split(EndReason),
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle { retry_at: Option<Instant> },
    Recording { since: Instant, match_seen: bool },
}

/// Decides when recordings start and end.
#[derive(Debug)]
pub struct Lifecycle {
    state: State,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            state: State::Idle { retry_at: None },
        }
    }
}

impl Lifecycle {
    /// What to do now. The lifecycle assumes it is done; report a failed start with [`Self::failed`].
    pub fn observe(&mut self, rules: &Rules, seen: &Observation) -> Option<Step> {
        let obstacle = if !rules.enabled {
            Some(EndReason::Disabled)
        } else if !seen.player_connected || (rules.require_human && !seen.human_connected) {
            Some(EndReason::NoPlayer)
        } else if !seen.has_room {
            Some(EndReason::DiskFull)
        } else {
            None
        };
        let in_match = seen.game_state == Some(GameState::InProgress);
        let fresh = State::Recording {
            since: seen.now,
            match_seen: in_match,
        };

        match &mut self.state {
            State::Idle { retry_at } => {
                if obstacle.is_some() || retry_at.is_some_and(|time| seen.now < time) {
                    return None;
                }
                self.state = fresh;
                Some(Step::Start)
            }
            State::Recording { since, match_seen } => {
                if let Some(reason) = obstacle {
                    self.state = State::Idle { retry_at: None };
                    return Some(Step::Finish(reason));
                }
                let split = if *match_seen && seen.game_state == Some(GameState::Warmup) {
                    Some(EndReason::MatchEnded)
                } else {
                    rules
                        .max_length
                        .filter(|&length| seen.now.duration_since(*since) >= length)
                        .map(|_| EndReason::MaxLength)
                };
                *match_seen |= in_match;
                split.map(|reason| {
                    self.state = fresh;
                    Step::Split(reason)
                })
            }
        }
    }

    /// The recording ended for a reason outside of these rules, like the end of the map.
    pub fn ended(&mut self) {
        self.state = State::Idle { retry_at: None };
    }

    /// The recording could not be started or continued.
    pub fn failed(&mut self, now: Instant) {
        self.state = State::Idle {
            retry_at: Some(now + RETRY_DELAY),
        };
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    const RULES: Rules = Rules {
        enabled: true,
        require_human: true,
        max_length: Some(Duration::from_secs(600)),
    };

    fn seen(start: Instant, seconds: u64, state: GameState) -> Observation {
        Observation {
            now: start + Duration::from_secs(seconds),
            game_state: Some(state),
            player_connected: true,
            human_connected: true,
            has_room: true,
        }
    }

    fn steps(states: &[GameState]) -> Vec<Option<Step>> {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        (0..)
            .zip(states)
            .map(|(second, &state)| lifecycle.observe(&RULES, &seen(start, second, state)))
            .collect()
    }

    #[test]
    fn warmup_and_match_share_a_recording_until_the_next_warmup() {
        use GameState::{Countdown, InProgress, Warmup};

        let steps = steps(&[Warmup, Countdown, InProgress, InProgress, Warmup, Warmup]);

        assert_eq!(
            steps,
            [
                Some(Step::Start),
                None,
                None,
                None,
                Some(Step::Split(EndReason::MatchEnded)),
                None
            ]
        );
    }

    #[test]
    fn an_aborted_countdown_keeps_the_recording() {
        use GameState::{Countdown, Warmup};

        let steps = steps(&[Warmup, Countdown, Warmup]);

        assert_eq!(steps, [Some(Step::Start), None, None]);
    }

    #[test]
    fn an_unknown_state_does_not_end_a_match() {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        lifecycle.observe(&RULES, &seen(start, 0, GameState::InProgress));

        let unknown = Observation {
            game_state: GameState::from_cvar(""),
            ..seen(start, 1, GameState::Warmup)
        };

        assert_eq!(lifecycle.observe(&RULES, &unknown), None);
    }

    #[rstest]
    #[case::disabled(Rules { enabled: false, ..RULES }, true, true, EndReason::Disabled)]
    #[case::empty(RULES, false, true, EndReason::NoPlayer)]
    #[case::full(RULES, true, false, EndReason::DiskFull)]
    fn obstacles_end_and_prevent_recordings(
        #[case] rules: Rules,
        #[case] human_connected: bool,
        #[case] has_room: bool,
        #[case] reason: EndReason,
    ) {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        lifecycle.observe(&RULES, &seen(start, 0, GameState::Warmup));
        let observation = Observation {
            human_connected,
            has_room,
            ..seen(start, 1, GameState::Warmup)
        };

        assert_eq!(
            lifecycle.observe(&rules, &observation),
            Some(Step::Finish(reason))
        );
        assert_eq!(lifecycle.observe(&rules, &observation), None);
    }

    #[test]
    fn bots_are_recorded_if_no_human_is_required() {
        let rules = Rules {
            require_human: false,
            ..RULES
        };
        let observation = Observation {
            human_connected: false,
            ..seen(Instant::now(), 0, GameState::Warmup)
        };

        assert_eq!(
            Lifecycle::default().observe(&rules, &observation),
            Some(Step::Start)
        );
    }

    #[rstest]
    #[case::a_human_required(true)]
    #[case::bots_allowed(false)]
    fn an_empty_server_is_not_recorded(#[case] require_human: bool) {
        let rules = Rules {
            require_human,
            ..RULES
        };
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        let empty = Observation {
            player_connected: false,
            human_connected: false,
            ..seen(start, 1, GameState::Warmup)
        };

        assert_eq!(lifecycle.observe(&rules, &empty), None);
        assert_eq!(
            lifecycle.observe(&rules, &seen(start, 2, GameState::Warmup)),
            Some(Step::Start)
        );
        assert_eq!(
            lifecycle.observe(&rules, &empty),
            Some(Step::Finish(EndReason::NoPlayer))
        );
    }

    #[test]
    fn long_recordings_are_split() {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        let mut at = |second| lifecycle.observe(&RULES, &seen(start, second, GameState::Warmup));

        assert_eq!(at(0), Some(Step::Start));
        assert_eq!(at(599), None);
        assert_eq!(at(600), Some(Step::Split(EndReason::MaxLength)));
        assert_eq!(at(1199), None);
    }

    #[test]
    fn after_a_failure_the_next_try_waits() {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        lifecycle.failed(start);
        let mut at = |second| lifecycle.observe(&RULES, &seen(start, second, GameState::Warmup));

        assert_eq!(at(59), None);
        assert_eq!(at(60), Some(Step::Start));
    }

    #[test]
    fn after_the_end_of_a_map_a_new_recording_starts_at_once() {
        let (mut lifecycle, start) = (Lifecycle::default(), Instant::now());
        lifecycle.observe(&RULES, &seen(start, 0, GameState::InProgress));

        lifecycle.ended();

        assert_eq!(
            lifecycle.observe(&RULES, &seen(start, 1, GameState::Warmup)),
            Some(Step::Start)
        );
    }
}
