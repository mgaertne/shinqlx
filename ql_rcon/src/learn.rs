//! Picks up what the server reports on its own: the usage of a command and the default and range of a cvar.

use crate::guide::meaning;

/// Something worth remembering that was found in a reply of the server.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Learned {
    Usage {
        command: String,
        text: String,
    },
    Factories(Vec<String>),
    /// A cvar that only changes with the next restart.
    Latched(String),
    /// A cvar that the server refuses to set.
    ReadOnly(String),
    Cvar {
        name: String,
        default: String,
        minimum: String,
        maximum: String,
    },
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
}

fn value(text: &str) -> String {
    text.trim_end_matches("^7").trim().to_string()
}

fn usage(line: &str) -> Option<Learned> {
    let prefix = line.get(..6)?;
    if !prefix.eq_ignore_ascii_case("usage:") {
        return None;
    }

    let text = line[6..].trim();
    let command = text.split_whitespace().next()?;
    is_name(command).then(|| Learned::Usage {
        command: command.to_lowercase(),
        text: text.to_string(),
    })
}

/// Parses `"name" is:"value" default:"value"`, optionally with `minimum:"value"` and `maximum:"value"`.
fn cvar(line: &str) -> Option<Learned> {
    let parts: Vec<&str> = line.split('"').collect();
    let name = parts.get(1).copied().filter(|name| is_name(name))?;
    if parts.first().is_none_or(|before| !before.trim().is_empty()) {
        return None;
    }

    let (mut default, mut minimum, mut maximum) = (None, String::new(), String::new());
    for pair in parts[2..].as_chunks::<2>().0 {
        match pair[0].trim().to_lowercase().as_str() {
            "default:" => default = Some(value(pair[1])),
            "minimum:" | "min:" => minimum = value(pair[1]),
            "maximum:" | "max:" => maximum = value(pair[1]),
            _ => (),
        }
    }

    Some(Learned::Cvar {
        name: name.to_string(),
        default: default?,
        minimum,
        maximum,
    })
}

/// What the current value in `"name" is:"value" ...` stands for, as far as the guide knows: the bits of a bit field or
/// the label of an enumeration.
pub(crate) fn explained(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split('"').collect();
    let name = parts.get(1).copied().filter(|name| is_name(name))?;
    if parts.first().is_none_or(|before| !before.trim().is_empty()) {
        return None;
    }

    let current = parts[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .find(|pair| pair[0].trim().eq_ignore_ascii_case("is:"))
        .map(|pair| pair[1])?;
    meaning(name, current).map(|text| format!("  = {text}"))
}

/// Parses `<cvar> will be changed to <value> upon restarting.`
fn latched(line: &str) -> Option<Learned> {
    let (name, rest) = line.split_once(" will be changed to ")?;
    (rest.ends_with(" upon restarting.") && is_name(name))
        .then(|| Learned::Latched(name.to_string()))
}

/// Parses `<cvar> is read only.`
fn read_only(line: &str) -> Option<Learned> {
    let name = line.strip_suffix(" is read only.")?;
    is_name(name).then(|| Learned::ReadOnly(name.to_string()))
}

/// Recognises a usage line or the answer to a cvar query; any other line yields `None`.
fn learn_line(line: &str) -> Option<Learned> {
    let line = line.trim();
    usage(line)
        .or_else(|| cvar(line))
        .or_else(|| latched(line))
        .or_else(|| read_only(line))
}

/// Reads the replies line by line; the list of factories that `map` prints spans several lines.
#[derive(Debug, Default)]
pub(crate) struct Learner {
    factories: Option<Vec<String>>,
}

impl Learner {
    pub(crate) fn feed(&mut self, line: &str) -> Option<Learned> {
        let line = line.trim();

        if let Some(factories) = self.factories.as_mut() {
            if is_name(line) {
                factories.push(line.to_string());
                return None;
            }
            let factories = self.factories.take().unwrap_or_default();
            return if factories.is_empty() {
                learn_line(line)
            } else {
                Some(Learned::Factories(factories))
            };
        }

        if line.eq_ignore_ascii_case("valid factories:") {
            self.factories = Some(Vec::new());
            return None;
        }

        learn_line(line)
    }
}

#[cfg(test)]
mod learn_tests {
    use super::{Learned, Learner, explained};

    #[test]
    fn a_cvar_answer_is_explained_when_the_guide_knows_the_value() {
        assert_eq!(
            explained("\"g_startingWeapons\" is:\"4100^7\" default:\"8193^7\"").as_deref(),
            Some("  = Shotgun + Chaingun")
        );
        assert_eq!(
            explained("\"g_gametype\" is:\"4^7\" default:\"0^7\"").as_deref(),
            Some("  = Clan Arena")
        );
        assert_eq!(
            explained("\"sv_hostname\" is:\"x^7\" default:\"y^7\""),
            None
        );
        assert_eq!(explained("something else \"g_gametype\" is:\"4\""), None);
    }

    #[test]
    fn usage_lines_are_learned_in_any_case() {
        assert_eq!(
            Learner::default().feed("usage: dir <directory> [extension]"),
            Some(Learned::Usage {
                command: "dir".to_string(),
                text: "dir <directory> [extension]".to_string()
            })
        );
        assert_eq!(
            Learner::default().feed("Usage: Kick <player name>kick all = kick everyone"),
            Some(Learned::Usage {
                command: "kick".to_string(),
                text: "Kick <player name>kick all = kick everyone".to_string()
            })
        );
    }

    #[test]
    fn cvar_answers_give_default_and_range() {
        assert_eq!(
            Learner::default().feed("\"sv_maxclients\" is:\"16^7\" default:\"8^7\""),
            Some(Learned::Cvar {
                name: "sv_maxclients".to_string(),
                default: "8".to_string(),
                minimum: String::new(),
                maximum: String::new(),
            })
        );
        assert_eq!(
            Learner::default().feed("\"g_x\" is:\"1\" default:\"0\" minimum:\"0\" maximum:\"5\""),
            Some(Learned::Cvar {
                name: "g_x".to_string(),
                default: "0".to_string(),
                minimum: "0".to_string(),
                maximum: "5".to_string(),
            })
        );
    }

    #[test]
    fn the_short_range_keys_are_understood() {
        assert_eq!(
            Learner::default().feed("\"sv_padPackets\" is:\"0\" default:\"0\" min:\"0\" max:\"3\""),
            Some(Learned::Cvar {
                name: "sv_padPackets".to_string(),
                default: "0".to_string(),
                minimum: "0".to_string(),
                maximum: "3".to_string(),
            })
        );
    }

    #[test]
    fn the_latched_message_names_the_cvar() {
        assert_eq!(
            Learner::default().feed("bot_enable will be changed to 1 upon restarting."),
            Some(Learned::Latched("bot_enable".to_string()))
        );
        assert_eq!(Learner::default().feed("say will be changed to soon"), None);
    }

    #[test]
    fn the_read_only_message_names_the_cvar() {
        assert_eq!(
            Learner::default().feed("sv_cheats is read only."),
            Some(Learned::ReadOnly("sv_cheats".to_string()))
        );
        assert_eq!(Learner::default().feed("the map is read only."), None);
    }

    #[test]
    fn the_factories_of_the_map_command_are_collected_until_the_empty_line() {
        let mut learner = Learner::default();

        assert_eq!(learner.feed("map (map) (factory)"), None);
        assert_eq!(learner.feed("Valid factories:"), None);
        assert_eq!(learner.feed("ca"), None);
        assert_eq!(learner.feed("ctf"), None);
        assert_eq!(
            learner.feed(""),
            Some(Learned::Factories(vec![
                "ca".to_string(),
                "ctf".to_string()
            ]))
        );
        assert_eq!(learner.feed("ffa"), None);
    }

    #[test]
    fn other_lines_are_not_learned() {
        assert_eq!(
            Learner::default().feed("zmq RCON command from abc: dir"),
            None
        );
        assert_eq!(Learner::default().feed("\"say\" hello \"there\""), None);
        assert_eq!(Learner::default().feed("usage:"), None);
        assert_eq!(Learner::default().feed(""), None);
    }
}
