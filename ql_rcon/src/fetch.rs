//! Collects the command and cvar names that the server reports for `cmdlist` and `cvarlist`.
use core::time::Duration;
use std::time::Instant;

/// The rcon commands that make the server list its names.
pub(crate) const FETCH_COMMANDS: [&str; 2] = ["cmdlist", "cvarlist"];

/// A server that never sends the end marker of `cvarlist` must not swallow lines forever.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// What to do with a received line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Feed {
    Show,
    Hide,
    Done(Vec<String>),
}

#[derive(Debug)]
pub(crate) enum FetchState {
    Off,
    Waiting,
    Collecting {
        names: Vec<String>,
        started: Instant,
    },
    Done,
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
}

fn finished(mut names: Vec<String>) -> Vec<String> {
    names.sort_unstable_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

impl FetchState {
    pub(crate) fn new(enabled: bool) -> Self {
        if enabled { Self::Waiting } else { Self::Off }
    }

    /// Returns `true` once, when the lists have to be requested.
    pub(crate) fn handshake_succeeded(&mut self) -> bool {
        if !matches!(self, Self::Waiting) {
            return false;
        }

        *self = Self::Collecting {
            names: Vec::new(),
            started: Instant::now(),
        };
        true
    }

    /// Takes the names of the lists out of `line`; every other line stays visible.
    pub(crate) fn feed(&mut self, line: &str) -> Feed {
        let Self::Collecting { names, .. } = self else {
            return Feed::Show;
        };

        let line = line.trim();
        let mut words = line.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (None, ..) => Feed::Hide,
            (Some(count), Some("cvar"), Some("indexes")) if count.parse::<u32>().is_ok() => {
                let names = finished(core::mem::take(names));
                *self = Self::Done;
                Feed::Done(names)
            }
            (Some(count), Some("total"), Some("cvars")) if count.parse::<u32>().is_ok() => {
                Feed::Hide
            }
            (Some(name), None, _) if is_name(name) => {
                names.push(name.to_string());
                Feed::Hide
            }
            (Some(name), Some(value), _)
                if is_name(name)
                    && line[name.len()..].trim().starts_with('"')
                    && line.ends_with('"')
                    && !value.is_empty() =>
            {
                names.push(name.to_string());
                Feed::Hide
            }
            _ => Feed::Show,
        }
    }

    /// Ends a collection whose end marker did not arrive in time and returns what was found.
    pub(crate) fn expired(&mut self) -> Option<Vec<String>> {
        let Self::Collecting { names, started } = self else {
            return None;
        };
        if started.elapsed() < FETCH_TIMEOUT {
            return None;
        }

        let names = finished(core::mem::take(names));
        *self = Self::Done;
        Some(names)
    }
}

#[cfg(test)]
mod fetch_tests {
    use super::{Feed, FetchState};

    fn collecting() -> FetchState {
        let mut state = FetchState::new(true);
        assert!(state.handshake_succeeded());
        state
    }

    #[test]
    fn a_disabled_fetch_requests_nothing_and_shows_everything() {
        let mut state = FetchState::new(false);

        assert!(!state.handshake_succeeded());
        assert_eq!(state.feed("status"), Feed::Show);
    }

    #[test]
    fn the_lists_are_requested_only_once() {
        let mut state = collecting();

        assert!(!state.handshake_succeeded());
    }

    #[test]
    fn lines_before_the_request_stay_visible() {
        let mut state = FetchState::new(true);

        assert_eq!(state.feed("map"), Feed::Show);
    }

    #[test]
    fn command_and_cvar_lines_are_hidden_and_collected() {
        let mut state = collecting();

        assert_eq!(state.feed("touchFile"), Feed::Hide);
        assert_eq!(state.feed(""), Feed::Hide);
        assert_eq!(state.feed("sv_cheats \"0\""), Feed::Hide);
        assert_eq!(state.feed("sv_hostname \"my server\""), Feed::Hide);
        assert_eq!(state.feed("530 total cvars"), Feed::Hide);
        assert_eq!(
            state.feed("530 cvar indexes"),
            Feed::Done(vec![
                "sv_cheats".to_string(),
                "sv_hostname".to_string(),
                "touchFile".to_string()
            ])
        );
    }

    #[test]
    fn other_lines_stay_visible_while_collecting() {
        let mut state = collecting();

        assert_eq!(state.feed("player joined the game"), Feed::Show);
        assert_eq!(state.feed("chat: hello \"there\""), Feed::Show);
    }

    #[test]
    fn names_are_sorted_and_unique_regardless_of_case() {
        let mut state = collecting();

        state.feed("map");
        state.feed("Map");
        state.feed("kick");

        assert_eq!(
            state.feed("1 cvar indexes"),
            Feed::Done(vec!["kick".to_string(), "map".to_string()])
        );
    }

    #[test]
    fn after_the_end_marker_everything_is_shown_again() {
        let mut state = collecting();
        state.feed("1 cvar indexes");

        assert_eq!(state.feed("map"), Feed::Show);
        assert!(!state.handshake_succeeded());
    }

    #[test]
    fn a_collection_that_just_started_has_not_expired() {
        let mut state = collecting();

        assert_eq!(state.expired(), None);
    }
}
