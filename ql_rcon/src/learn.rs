//! Picks up what the server reports on its own: the usage of a command and the default and range of a cvar.

/// Something worth remembering that was found in a reply of the server.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Learned {
    Usage {
        command: String,
        text: String,
    },
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

/// Recognises a usage line or the answer to a cvar query; any other line yields `None`.
pub(crate) fn learn_line(line: &str) -> Option<Learned> {
    let line = line.trim();
    usage(line).or_else(|| cvar(line))
}

#[cfg(test)]
mod learn_tests {
    use super::{Learned, learn_line};

    #[test]
    fn usage_lines_are_learned_in_any_case() {
        assert_eq!(
            learn_line("usage: dir <directory> [extension]"),
            Some(Learned::Usage {
                command: "dir".to_string(),
                text: "dir <directory> [extension]".to_string()
            })
        );
        assert_eq!(
            learn_line("Usage: Kick <player name>kick all = kick everyone"),
            Some(Learned::Usage {
                command: "kick".to_string(),
                text: "Kick <player name>kick all = kick everyone".to_string()
            })
        );
    }

    #[test]
    fn cvar_answers_give_default_and_range() {
        assert_eq!(
            learn_line("\"sv_maxclients\" is:\"16^7\" default:\"8^7\""),
            Some(Learned::Cvar {
                name: "sv_maxclients".to_string(),
                default: "8".to_string(),
                minimum: String::new(),
                maximum: String::new(),
            })
        );
        assert_eq!(
            learn_line("\"g_x\" is:\"1\" default:\"0\" minimum:\"0\" maximum:\"5\""),
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
            learn_line("\"sv_padPackets\" is:\"0\" default:\"0\" min:\"0\" max:\"3\""),
            Some(Learned::Cvar {
                name: "sv_padPackets".to_string(),
                default: "0".to_string(),
                minimum: "0".to_string(),
                maximum: "3".to_string(),
            })
        );
    }

    #[test]
    fn other_lines_are_not_learned() {
        assert_eq!(learn_line("zmq RCON command from abc: dir"), None);
        assert_eq!(learn_line("\"say\" hello \"there\""), None);
        assert_eq!(learn_line("usage:"), None);
        assert_eq!(learn_line(""), None);
    }
}
