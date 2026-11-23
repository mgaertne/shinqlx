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
    ShowAndDone(Fetched),
    Replace(String),
}

/// What the server reported: command and cvar names, and map names without the `.bsp`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) names: Vec<String>,
    pub(crate) maps: Vec<String>,
    pub(crate) flags: Vec<(String, String)>,
}

impl Fetched {
    fn sorted(mut self) -> Self {
        self.names = finished(self.names);
        self.maps = finished(self.maps);
        self.flags
            .sort_unstable_by_key(|(name, _)| name.to_lowercase());
        self.flags.dedup_by_key(|(name, _)| name.to_lowercase());
        self
    }
}

#[derive(Debug)]
pub(crate) enum FetchState {
    Off,
    Waiting,
    Collecting {
        found: Fetched,
        started: Instant,
        last_line: Instant,
        visible: bool,
        pending_flags: String,
    },
    Done,
}

/// What the flag letters of a `cvarlist` stand for, in the order of the server's flag slots.
pub(crate) const FLAG_LEGEND: &str = "Flags: S serverinfo, U userinfo, R read-only, I init only, A archived, L latched, C cheat, T temporary (help cvarflags)";

/// The cvar line with its flags at the end, `sv_hostname "x"  [SA]`; a cvar without flags stays as it is.
fn with_flags(line: &str, flags: &str) -> String {
    if flags.is_empty() {
        line.to_string()
    } else {
        format!("{line}  [{flags}]")
    }
}

/// A list typed by the user has no end marker (`cmdlist`, `dir`), so it ends after this much silence.
const PASSIVE_IDLE: Duration = Duration::from_millis(1500);

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
}

/// The server sends the flags of a cvar (`S`, `A`, `L` and so on) as lines of one letter in front of its name.
fn is_flag(text: &str) -> bool {
    text.len() == 1 && text.starts_with(|c: char| c.is_ascii_uppercase())
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
            last_line: Instant::now(),
            visible: false,
            pending_flags: String::new(),
        };
        true
    }

    /// Starts reading along when the user types one of the list commands; the reply stays visible.
    pub(crate) fn user_sent(&mut self, line: &str) {
        if !matches!(self, Self::Off | Self::Done) || !FETCH_COMMANDS.contains(&line.trim()) {
            return;
        }

        *self = Self::Collecting {
            found: Fetched::default(),
            started: Instant::now(),
            last_line: Instant::now(),
            visible: true,
            pending_flags: String::new(),
        };
    }

    /// Takes the names of the lists out of `line`; every other line stays visible.
    pub(crate) fn feed(&mut self, line: &str) -> Feed {
        let Self::Collecting {
            found,
            visible,
            last_line,
            pending_flags,
            ..
        } = self
        else {
            return Feed::Show;
        };

        let hidden = if *visible { Feed::Show } else { Feed::Hide };
        let line = line.trim();
        let mut words = line.split_whitespace();
        let feed = match (words.next(), words.next(), words.next()) {
            (None, ..) => Feed::Hide,
            (Some("Directory"), Some("of"), _) => hidden,
            (Some(dashes), None, _) if dashes.chars().all(|c| c == '-') => hidden,
            (Some(file), None, _) if file.strip_suffix(".bsp").is_some_and(is_name) => {
                found.maps.push(file.trim_end_matches(".bsp").to_string());
                hidden
            }
            (Some(count), Some("cvar"), Some("indexes")) if count.parse::<u32>().is_ok() => {
                let show = *visible;
                let fetched = core::mem::take(found).sorted();
                *self = Self::Done;
                return if show {
                    Feed::ShowAndDone(fetched)
                } else {
                    Feed::Done(fetched)
                };
            }
            (Some(count), Some("total"), Some("cvars")) if count.parse::<u32>().is_ok() => hidden,
            (Some(flag), None, _) if is_flag(flag) => {
                pending_flags.push_str(flag);
                Feed::Hide
            }
            (Some(name), None, _) if is_name(name) => {
                pending_flags.clear();
                found.names.push(name.to_string());
                hidden
            }
            (Some(name), Some(value), _)
                if is_name(name)
                    && line[name.len()..].trim().starts_with('"')
                    && line.ends_with('"')
                    && !value.is_empty() =>
            {
                let flags = core::mem::take(pending_flags);
                found.names.push(name.to_string());
                if !flags.is_empty() {
                    found.flags.push((name.to_string(), flags.clone()));
                }
                if *visible {
                    Feed::Replace(with_flags(line, &flags))
                } else {
                    Feed::Hide
                }
            }
            _ => Feed::Show,
        };
        *last_line = Instant::now();
        feed
    }

    /// Ends a collection whose end marker did not arrive in time and returns what was found.
    pub(crate) fn expired(&mut self) -> Option<Fetched> {
        let Self::Collecting {
            found,
            started,
            last_line,
            visible,
            ..
        } = self
        else {
            return None;
        };
        let timed_out = if *visible {
            last_line.elapsed() >= PASSIVE_IDLE
        } else {
            started.elapsed() >= FETCH_TIMEOUT
        };
        if !timed_out {
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
                flags: Vec::new(),
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
                flags: Vec::new(),
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
                flags: Vec::new(),
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
    fn the_flag_lines_in_front_of_a_cvar_are_not_names() {
        let mut state = collecting();

        for line in [
            "S",
            "",
            "A",
            "sv_hostname \"A New Server\"",
            "L",
            "bot_enable \"0\"",
        ] {
            assert_eq!(state.feed(line), Feed::Hide, "{line}");
        }
        assert_eq!(
            state.feed("2 cvar indexes"),
            Feed::Done(Fetched {
                names: vec!["bot_enable".to_string(), "sv_hostname".to_string()],
                maps: Vec::new(),
                flags: vec![
                    ("bot_enable".to_string(), "L".to_string()),
                    ("sv_hostname".to_string(), "SA".to_string()),
                ],
            })
        );
    }

    #[test]
    fn flags_do_not_carry_over_from_a_command_to_the_next_cvar() {
        let mut state = collecting();

        for line in ["S", "addbot", "bot_enable \"0\""] {
            state.feed(line);
        }

        assert_eq!(
            state.feed("1 cvar indexes"),
            Feed::Done(Fetched {
                names: vec!["addbot".to_string(), "bot_enable".to_string()],
                maps: Vec::new(),
                flags: Vec::new(),
            })
        );
    }

    #[test]
    fn a_list_typed_by_the_user_is_read_along_and_stays_visible() {
        let mut state = FetchState::new(false);
        state.user_sent("cvarlist");

        assert_eq!(state.feed("S"), Feed::Hide);
        assert_eq!(state.feed("A"), Feed::Hide);
        assert_eq!(
            state.feed("sv_hostname \"A New Server\""),
            Feed::Replace("sv_hostname \"A New Server\"  [SA]".to_string())
        );
        assert_eq!(
            state.feed("sv_cheats \"0\""),
            Feed::Replace("sv_cheats \"0\"".to_string())
        );
        assert_eq!(state.feed("12 total cvars"), Feed::Show);
        assert_eq!(
            state.feed("12 cvar indexes"),
            Feed::ShowAndDone(Fetched {
                names: vec!["sv_cheats".to_string(), "sv_hostname".to_string()],
                maps: Vec::new(),
                flags: vec![("sv_hostname".to_string(), "SA".to_string())],
            })
        );
    }

    #[test]
    fn the_eight_flag_slots_of_the_server_come_before_the_cvar_line() {
        let mut state = FetchState::new(false);
        state.user_sent("cvarlist");
        let mut shown = Vec::new();

        for (flags, line) in [
            (["", "", "R", "", "", "", "", ""], " net_port \"27970\""),
            (
                ["S", "", "", "", "A", "", "", ""],
                " sv_hostname \"leaktest-A\"",
            ),
            (["", "", "", "", "", "", "", ""], " sv_cheats \"0\""),
        ] {
            for slot in flags {
                match state.feed(if slot.is_empty() { " " } else { slot }) {
                    Feed::Hide => (),
                    other => shown.push(format!("{other:?}")),
                }
            }
            if let Feed::Replace(text) = state.feed(line) {
                shown.push(text);
            }
        }

        assert_eq!(
            shown,
            [
                "net_port \"27970\"  [R]",
                "sv_hostname \"leaktest-A\"  [SA]",
                "sv_cheats \"0\"",
            ]
        );
    }

    #[test]
    fn empty_lines_of_a_list_typed_by_the_user_are_not_shown() {
        let mut state = FetchState::new(false);
        state.user_sent("cvarlist");

        assert_eq!(state.feed(""), Feed::Hide);
    }

    #[test]
    fn other_commands_typed_by_the_user_start_nothing() {
        let mut state = FetchState::new(false);
        state.user_sent("status");

        assert_eq!(state.feed("touchFile"), Feed::Show);
        assert_eq!(state.expired(), None);
    }

    #[test]
    fn a_list_without_end_marker_ends_after_the_silence() {
        let mut state = FetchState::new(false);
        state.user_sent("cmdlist");
        state.feed("kick");

        assert_eq!(state.expired(), None);
        if let FetchState::Collecting { last_line, .. } = &mut state {
            *last_line -= core::time::Duration::from_secs(2);
        }
        assert_eq!(
            state.expired(),
            Some(Fetched {
                names: vec!["kick".to_string()],
                maps: Vec::new(),
                flags: Vec::new(),
            })
        );
    }

    #[test]
    fn a_collection_that_just_started_has_not_expired() {
        let mut state = collecting();

        assert_eq!(state.expired(), None);
    }
}
