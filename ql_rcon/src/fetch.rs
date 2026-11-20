//! Collects the command and cvar names that the server reports for `cmdlist` and `cvarlist`.
use core::time::Duration;
use std::time::Instant;

/// The rcon commands that make the server list its maps, commands and cvars, in the order the lists arrive.
pub(crate) const FETCH_COMMANDS: [&str; 3] = ["dir maps .bsp", "cmdlist", "cvarlist"];

/// A server that never sends the end marker of `cvarlist` must not swallow lines forever.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// What to do with a received line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Feed {
    Show,
    Hide,
    Done(Fetched),
}

/// What the server reported: command and cvar names, and map names without the `.bsp`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) names: Vec<String>,
    pub(crate) maps: Vec<String>,
}

impl Fetched {
    fn sorted(mut self) -> Self {
        self.names = finished(self.names);
        self.maps = finished(self.maps);
        self
    }
}

#[derive(Debug)]
pub(crate) enum FetchState {
    Off,
    Waiting,
    Collecting { found: Fetched, started: Instant },
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
            found: Fetched::default(),
            started: Instant::now(),
        };
        true
    }

    /// Takes the names of the lists out of `line`; every other line stays visible.
    pub(crate) fn feed(&mut self, line: &str) -> Feed {
        let Self::Collecting { found, .. } = self else {
            return Feed::Show;
        };

        let line = line.trim();
        let mut words = line.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (None, ..) => Feed::Hide,
            (Some("Directory"), Some("of"), _) => Feed::Hide,
            (Some(dashes), None, _) if dashes.chars().all(|c| c == '-') => Feed::Hide,
            (Some(file), None, _) if file.strip_suffix(".bsp").is_some_and(is_name) => {
                found.maps.push(file.trim_end_matches(".bsp").to_string());
                Feed::Hide
            }
            (Some(count), Some("cvar"), Some("indexes")) if count.parse::<u32>().is_ok() => {
                let fetched = core::mem::take(found).sorted();
                *self = Self::Done;
                Feed::Done(fetched)
            }
            (Some(count), Some("total"), Some("cvars")) if count.parse::<u32>().is_ok() => {
                Feed::Hide
            }
            (Some(name), None, _) if is_name(name) => {
                found.names.push(name.to_string());
                Feed::Hide
            }
            (Some(name), Some(value), _)
                if is_name(name)
                    && line[name.len()..].trim().starts_with('"')
                    && line.ends_with('"')
                    && !value.is_empty() =>
            {
                found.names.push(name.to_string());
                Feed::Hide
            }
            _ => Feed::Show,
        }
    }

    /// Ends a collection whose end marker did not arrive in time and returns what was found.
    pub(crate) fn expired(&mut self) -> Option<Fetched> {
        let Self::Collecting { found, started } = self else {
            return None;
        };
        if started.elapsed() < FETCH_TIMEOUT {
            return None;
        }

        let fetched = core::mem::take(found).sorted();
        *self = Self::Done;
        Some(fetched)
    }
}

#[cfg(test)]
mod fetch_tests {
    use super::{Feed, FetchState, Fetched};

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
            Feed::Done(Fetched {
                names: vec![
                    "sv_cheats".to_string(),
                    "sv_hostname".to_string(),
                    "touchFile".to_string()
                ],
                maps: Vec::new(),
            })
        );
    }

    #[test]
    fn map_files_are_hidden_and_collected_without_the_extension() {
        let mut state = collecting();

        assert_eq!(state.feed("Directory of maps .bsp"), Feed::Hide);
        assert_eq!(state.feed("---------------"), Feed::Hide);
        assert_eq!(state.feed("zen.bsp"), Feed::Hide);
        assert_eq!(state.feed("aerowalk.bsp"), Feed::Hide);
        assert_eq!(state.feed("notes.txt"), Feed::Show);
        assert_eq!(
            state.feed("1 cvar indexes"),
            Feed::Done(Fetched {
                names: Vec::new(),
                maps: vec!["aerowalk".to_string(), "zen".to_string()],
            })
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
            Feed::Done(Fetched {
                names: vec!["kick".to_string(), "map".to_string()],
                maps: Vec::new(),
            })
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
