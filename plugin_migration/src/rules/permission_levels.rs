use super::{Rule, RuleOutput};
use crate::{
    edit::Edit,
    source::Source,
    tree::{argument, imported_module_name, method_call, walk},
};

const LEVELS: [&str; 6] = [
    "PLAYER",
    "MODERATOR",
    "GAME_ADMIN",
    "COMMUNITY_ADMIN",
    "SERVER_ADMIN",
    "GOD",
];

/// Names the permission level in the `permission` argument of `add_command` if it is a number from
/// 0 to 5.
pub(crate) struct PermissionLevels;

impl Rule for PermissionLevels {
    fn name(&self) -> &'static str {
        "permission-levels"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        let Some(module) = imported_module_name(source) else {
            return output;
        };
        walk(source.root(), &mut |node| {
            let Some(call) = method_call(node) else {
                return;
            };
            if source.text_of(call.name) != "add_command" {
                return;
            }
            let level = argument(call.arguments, source, 2, "permission")
                .filter(|permission| permission.kind() == "integer")
                .and_then(|permission| {
                    let level = source.text_of(permission).parse::<usize>().ok()?;
                    Some((permission, LEVELS.get(level)?))
                });
            if let Some((permission, name)) = level {
                output.edits.push(Edit::replace(
                    permission.byte_range(),
                    format!("{module}.Permissions.{name}"),
                ));
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::PermissionLevels;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> String {
        let source = Source::parse(text).expect("the test source is valid python");
        apply_edits(text, PermissionLevels.apply(&source).edits).expect("the edits do not overlap")
    }

    #[rstest]
    #[case(
        "import minqlx\nself.add_command('a', self.c, 2)\n",
        "import minqlx\nself.add_command('a', self.c, minqlx.Permissions.GAME_ADMIN)\n"
    )]
    #[case(
        "import minqlx\nself.add_command('a', self.c, permission=5, usage='x')\n",
        "import minqlx\nself.add_command('a', self.c, permission=minqlx.Permissions.GOD, usage='x')\n"
    )]
    #[case(
        "import minqlx\nself.add_command('a', self.c)\n",
        "import minqlx\nself.add_command('a', self.c)\n"
    )]
    #[case(
        "import minqlx\nself.add_command('a', self.c, 6)\n",
        "import minqlx\nself.add_command('a', self.c, 6)\n"
    )]
    #[case(
        "import minqlx\nself.add_command('a', self.c, level)\n",
        "import minqlx\nself.add_command('a', self.c, level)\n"
    )]
    #[case(
        "from minqlx import Plugin\nself.add_command('a', self.c, 2)\n",
        "from minqlx import Plugin\nself.add_command('a', self.c, 2)\n"
    )]
    fn names_the_permission_level(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), expected);
    }
}
