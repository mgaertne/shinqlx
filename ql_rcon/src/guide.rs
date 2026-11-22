//! What the console guide at https://www.euere.eu/ql/ says about commands and cvars.

/// `cvar<TAB>value<TAB>label<TAB>e|b`: one value of an enumeration (`e`) or one bit of a bit field (`b`).
const ENUMS: &str = include_str!("cvar_enums.txt");
/// `cvar<TAB>flags`, the letters of the guide: A archive, C cheat, I init, L latched, R read-only, S server info, T saved
/// to the QL database, U user info, W write-protected.
const FLAGS: &str = include_str!("cvar_flags.txt");
/// `name<TAB>first line of the description`.
const HELP: &str = include_str!("help.txt");

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

/// The first line of the description of a command or cvar.
pub(crate) fn help_for(name: &str) -> Option<&'static str> {
    HELP.lines().find_map(|line| {
        line.split_once('\t')
            .filter(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, help)| help)
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
    }

    #[test]
    fn vote_flags_name_the_disabled_votes() {
        assert_eq!(
            meaning("g_voteFlags", "14054").as_deref(),
            Some(
                "map_restart + nextmap + timelimit + fraglimit + shuffle + cointoss/random + loadouts + \
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
}
