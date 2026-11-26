//! What the console guide at <https://www.euere.eu/ql/> says about commands and cvars.

/// `cvar<TAB>value<TAB>label<TAB>e|b`: one value of an enumeration (`e`) or one bit of a bit field (`b`).
const ENUMS: &str = include_str!("cvar_enums.txt");
/// `cvar<TAB>flags`, the letters of the guide: A archive, C cheat, I init, L latched, R read-only, S server info, T saved
/// to the QL database, U user info, W write-protected.
const FLAGS: &str = include_str!("cvar_flags.txt");
/// `name<TAB>first line of the description`.
const HELP: &str = include_str!("help.txt");
/// `name<TAB>description` of the `qlx_` cvars of shinqlx.
const QLX_CVARS: &str = include_str!("qlx_cvars.txt");
/// `name<TAB>description` of the commands of the plugins that shinqlx brings, without the command prefix.
const PLUGIN_COMMANDS: &str = include_str!("plugin_commands.txt");

/// The text of `help cvarflags`, which the console answers itself instead of asking the server.
const CVAR_FLAGS_HELP: [&str; 9] = [
    "The letters at the end of a cvar in cvarlist, in the order of the server:",
    "  S  serverinfo  sent in response to front end requests, such as server queries",
    "  U  userinfo    sent to the server on connect or change",
    "  R  read-only   display only, cannot be set by the user at all",
    "  I  init only   cannot be changed from the console, but can be set on the command line",
    "  A  archived    saved to the config file",
    "  L  latched     changes only after a map change or restart",
    "  C  cheat       cannot be changed while cheats are disabled",
    "  T  temporary   can be set while cheats are disabled, but is not archived",
];

/// A command of shinqlx, which the console guide of the game does not know.
struct ShinqlxCommand {
    name: &'static str,
    short: &'static str,
    help: &'static [&'static str],
}

const SHINQLX_COMMANDS: [ShinqlxCommand; 2] = [
    ShinqlxCommand {
        name: "plugincmds",
        short: "Lists the commands of the loaded plugins of a shinqlx server, as they are typed after qlx.",
        help: &[
            "plugincmds [plugin]",
            "  Lists the commands of the loaded plugins of a shinqlx server, \
            sorted by plugin and name, with the",
            "  permission and the usage of each. The commands are written \
            as they are typed after qlx, with the",
            "  prefix where they need it. The last line is the count.",
            "  plugincmds ban   lists only the commands of the plugin ban",
            "  qlx !version     runs one of the listed commands",
        ],
    },
    ShinqlxCommand {
        name: "qlx",
        short: "Runs a command of a plugin as if the owner typed it in the console, e.g. qlx !version.",
        help: &[
            "qlx <command> [arguments]",
            "  Runs a command of a plugin of a shinqlx server as if \
            the owner typed it in the console channel.",
            "  A command that needs the prefix is typed with it, \
            e.g. qlx !version. plugincmds lists them.",
        ],
    },
];

/// The names of the commands of shinqlx, which the console offers like those of the game.
pub(crate) fn shinqlx_command_names<'a>() -> impl Iterator<Item = &'a str> {
    SHINQLX_COMMANDS.iter().map(|command| command.name)
}

/// The answer to a line the console knows without the server:`help cvarflags` and `help` for a shinqlx command.
pub(crate) fn local_help(line: &str) -> Option<&'static [&'static str]> {
    let words: Vec<String> = line.split_whitespace().map(str::to_lowercase).collect();
    let [help, topic] = words.as_slice() else {
        return None;
    };
    if help != "help" {
        return None;
    }
    match topic.as_str() {
        "cvarflags" | "flags" => Some(CVAR_FLAGS_HELP.as_slice()),
        name => SHINQLX_COMMANDS
            .iter()
            .find(|command| command.name == name)
            .map(|command| command.help),
    }
}

/// The values or bits of a cvar with their meaning, in the order of the guide.
pub(crate) fn value_labels(cvar: &str) -> Vec<(&'static str, &'static str)> {
    ENUMS
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter_map(|fields| match fields.as_slice() {
            [name, value, label, _kind] if name.eq_ignore_ascii_case(cvar) => {
                Some((*value, *label))
            }
            _ => None,
        })
        .collect()
}

/// What a value of a cvar stands for: the names of the bits that are set for a bit field, the label for an enumeration.
pub(crate) fn meaning(cvar: &str, value: &str) -> Option<String> {
    let value: i64 = value.trim().trim_end_matches("^7").trim().parse().ok()?;
    let rows: Vec<(i64, &str, &str)> = ENUMS
        .lines()
        .filter_map(
            |line| match line.split('\t').collect::<Vec<_>>().as_slice() {
                [name, bit, label, kind] if name.eq_ignore_ascii_case(cvar) => {
                    Some((bit.parse().ok()?, *label, *kind))
                }
                _ => None,
            },
        )
        .collect();

    let bits: Vec<_> = rows.iter().filter(|(_, _, kind)| *kind == "b").collect();
    if bits.is_empty() {
        return rows
            .iter()
            .find(|(known, _, _)| *known == value)
            .map(|(_, label, _)| (*label).to_string());
    }

    let mut names: Vec<String> = bits
        .iter()
        .filter(|(bit, _, _)| *bit != 0 && value & bit == *bit)
        .map(|(_, label, _)| (*label).to_string())
        .collect();
    let known = bits.iter().fold(0, |known, (bit, _, _)| known | bit);
    if value & !known != 0 {
        names.push(format!("unknown bits {:#x}", value & !known));
    }
    Some(if names.is_empty() {
        "no bit set".to_string()
    } else {
        names.join(" + ")
    })
}

/// The flags of a cvar as the guide lists them, for example `AL`.
pub(crate) fn flags_for(cvar: &str) -> &'static str {
    FLAGS
        .lines()
        .find_map(|line| {
            line.split_once('\t')
                .filter(|(known, _)| known.eq_ignore_ascii_case(cvar))
                .map(|(_, flags)| flags)
        })
        .unwrap_or_default()
}

fn description_in(table: &'static str, name: &str) -> Option<&'static str> {
    table.lines().find_map(|line| {
        line.split_once('\t')
            .filter(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, help)| help)
    })
}

/// What a command of a standard plugin does; `!blue` and `blue` find the same text.
pub(crate) fn plugin_command_help(name: &str) -> Option<&'static str> {
    description_in(
        PLUGIN_COMMANDS,
        name.trim_start_matches(|c: char| !c.is_alphanumeric()),
    )
}

/// The first line of the description of a command or cvar.
pub(crate) fn help_for(name: &str) -> Option<&'static str> {
    description_in(HELP, name)
        .or_else(|| description_in(QLX_CVARS, name))
        .or_else(|| {
            SHINQLX_COMMANDS
                .iter()
                .find(|command| command.name.eq_ignore_ascii_case(name))
                .map(|command| command.short)
        })
}

#[cfg(test)]
mod guide_tests {
    use super::{flags_for, help_for, meaning, value_labels};

    #[test]
    fn bit_fields_are_broken_down_into_their_bits() {
        assert_eq!(
            meaning("g_startingWeapons", "4100").as_deref(),
            Some("Shotgun + Chaingun")
        );
        assert_eq!(
            meaning("g_startingWeapons", "16384").as_deref(),
            Some("unknown bits 0x4000")
        );
        assert_eq!(
            meaning("g_startingWeapons", "0").as_deref(),
            Some("no bit set")
        );
        assert_eq!(
            meaning("G_DROPCMDS", "5^7").as_deref(),
            Some("flags (dropflag) + weapons (dropweapon)")
        );
    }

    #[test]
    fn the_data_has_no_placeholders_of_the_guide() {
        assert!(!super::ENUMS.contains("???"));
        assert!(!super::HELP.contains("???"));
        assert!(!include_str!("cvar_values.txt").contains("???"));
    }

    #[test]
    fn the_texts_have_no_runs_of_spaces_from_the_html() {
        for text in [super::ENUMS, super::HELP] {
            assert!(
                text.lines()
                    .all(|line| !line.contains("  ") && !line.ends_with(' ')),
                "a line has two spaces in a row or a trailing space"
            );
            assert!(
                !text.contains("&rsquo;") && !text.contains("&lsquo;") && !text.contains("&amp;"),
                "an html entity is left"
            );
        }
        assert!(super::ENUMS.lines().all(|line| !line.ends_with('"')));
        assert!(
            super::HELP
                .lines()
                .all(|line| !line.contains("\t\"") && !line.ends_with("More Details"))
        );
    }

    #[test]
    fn vote_flags_name_the_disabled_votes() {
        assert_eq!(
            meaning("g_voteFlags", "14054").as_deref(),
            Some(
                "map_restart + nextmap + timelimit + fraglimit + shuffle + cointoss/random + \
                loadouts + \
                 ammo (global) + timers (item)"
            )
        );
    }

    #[test]
    fn enumerations_are_named_and_unknown_values_are_not() {
        assert_eq!(meaning("g_gametype", "4").as_deref(), Some("Clan Arena"));
        assert_eq!(meaning("g_gametype", "99"), None);
        assert_eq!(meaning("g_gametype", "x"), None);
        assert_eq!(meaning("no_such_cvar", "1"), None);
    }

    #[test]
    fn flags_are_found_in_any_case() {
        assert_eq!(flags_for("SV_CHEATS"), "R");
        assert_eq!(flags_for("no_such_cvar"), "");
    }

    #[test]
    fn enumerations_name_their_values() {
        let labels = value_labels("G_GAMETYPE");

        assert_eq!(labels.first(), Some(&("0", "Free for All")));
        assert!(labels.contains(&("4", "Clan Arena")), "{labels:?}");
    }

    #[test]
    fn bit_fields_name_their_bits() {
        assert!(value_labels("g_voteFlags").contains(&("2", "map_restart")));
    }

    #[test]
    fn labels_that_the_guide_gives_as_usage_are_found() {
        assert_eq!(
            value_labels("g_logSync"),
            [
                ("0", "overwrite existing files"),
                ("1", "write to the end of an existing file.")
            ]
        );
    }

    #[test]
    fn unknown_names_have_neither_labels_nor_help() {
        assert!(value_labels("no_such_cvar").is_empty());
        assert_eq!(help_for("no_such_cvar"), None);
    }

    #[test]
    fn commands_and_cvars_have_a_description() {
        assert_eq!(
            help_for("timelimit"),
            Some("Sets the time limit on your server.")
        );
        assert!(help_for("kick").is_some());
    }

    #[test]
    fn plugin_commands_are_found_with_and_without_the_prefix() {
        assert_eq!(
            super::plugin_command_help("!red"),
            super::plugin_command_help("red")
        );
        assert!(super::plugin_command_help("RED").is_some());
        assert_eq!(super::plugin_command_help("no_such_command"), None);
    }

    #[test]
    fn every_line_of_the_shinqlx_tables_has_a_name_and_a_text() {
        [super::QLX_CVARS, super::PLUGIN_COMMANDS]
            .iter()
            .flat_map(|table| table.lines())
            .for_each(|line| {
                assert!(
                    line.split_once('\t')
                        .is_some_and(|(name, text)| !name.is_empty() && !text.is_empty()),
                    "{line}"
                );
            });
        assert!(
            super::QLX_CVARS
                .lines()
                .all(|line| line.starts_with("qlx_"))
        );
    }

    #[test]
    fn the_commands_of_shinqlx_have_a_description_and_a_local_help() {
        assert!(help_for("plugincmds").is_some_and(|text| text.contains("plugins")));
        assert!(help_for("QLX").is_some());
        assert!(super::local_help("help plugincmds").is_some_and(|lines| lines.len() > 2));
        assert!(super::local_help("Help  qlx").is_some());
    }

    #[test]
    fn help_of_other_commands_goes_to_the_server() {
        assert_eq!(super::local_help("help kick"), None);
        assert_eq!(super::local_help("help"), None);
        assert_eq!(super::local_help("plugincmds"), None);
    }
}
