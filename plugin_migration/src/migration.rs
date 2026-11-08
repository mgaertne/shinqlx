use crate::{
    edit::apply_edits,
    rules::{Rule, Warning},
    source::Source,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Migration {
    Unchanged,
    Rewritten { text: String, edits: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileMigration {
    pub(crate) migration: Migration,
    pub(crate) warnings: Vec<Warning>,
}

impl FileMigration {
    fn unchanged(warnings: Vec<Warning>) -> Self {
        Self {
            migration: Migration::Unchanged,
            warnings,
        }
    }
}

/// Runs all rules one after another on `text`. A file that cannot be parsed, or that a rule would
/// break, stays unchanged.
pub(crate) fn migrate(text: &str, rules: &[Box<dyn Rule>]) -> FileMigration {
    let mut current = text.to_owned();
    let mut warnings = Vec::new();
    let mut edit_count = 0;

    for rule in rules {
        let output = {
            let Some(source) = Source::parse(&current) else {
                let message = if current == text {
                    "the file is not understood, it was kept unchanged".to_string()
                } else {
                    format!(
                        "rule {} produced invalid python, the file was kept unchanged",
                        rule.name()
                    )
                };
                warnings.push(Warning::new(0, message));
                return FileMigration::unchanged(warnings);
            };
            rule.apply(&source)
        };
        warnings.extend(output.warnings);

        if output.edits.is_empty() {
            continue;
        }
        edit_count += output.edits.len();
        let Ok(rewritten) = apply_edits(&current, output.edits) else {
            warnings.push(Warning::new(
                0,
                format!(
                    "rule {} produced overlapping edits, the file was kept unchanged",
                    rule.name()
                ),
            ));
            return FileMigration::unchanged(warnings);
        };
        current = rewritten;
    }

    if current == text {
        return FileMigration::unchanged(warnings);
    }
    FileMigration {
        migration: Migration::Rewritten {
            text: current,
            edits: edit_count,
        },
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{Migration, migrate};
    use crate::rules::default_rules;

    #[test]
    fn plugin_without_matches_is_unchanged() {
        let result = migrate("import os\n\nprint(os.name)\n", &default_rules());
        assert_eq!(result.migration, Migration::Unchanged);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn plugin_with_matches_is_rewritten() {
        let result = migrate(
            "import minqlx\n\nminqlx.log_exception()\n",
            &default_rules(),
        );
        assert_eq!(
            result.migration,
            Migration::Rewritten {
                text: "import shinqlx\n\nshinqlx.log_exception()\n".to_string(),
                edits: 2
            }
        );
    }

    #[test]
    fn unparsable_plugin_is_unchanged_with_a_warning() {
        let result = migrate("import minqlx\ndef broken(:\n", &default_rules());
        assert_eq!(result.migration, Migration::Unchanged);
        assert_eq!(result.warnings.len(), 1);
    }
}
