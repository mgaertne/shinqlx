//! What the console guide at https://www.euere.eu/ql/ says about commands and cvars.

/// `cvar<TAB>value<TAB>label<TAB>e|b`: one value of an enumeration (`e`) or one bit of a bit field (`b`).
const ENUMS: &str = include_str!("cvar_enums.txt");
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
    use super::{help_for, value_labels};

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
