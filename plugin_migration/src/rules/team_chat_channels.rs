use std::collections::HashSet;

use super::{Rule, RuleOutput, Warning};
use crate::{
    edit::Edit,
    source::{Source, line_of},
    tree::walk,
};

/// The classes of minqlx that shinqlx replaced by `TeamChatChannel` and the constant of the same channel.
const CHANNEL_CLASSES: [(&str, &str, &str); 4] = [
    ("RedTeamChatChannel", "red", "RED_TEAM_CHAT_CHANNEL"),
    ("BlueTeamChatChannel", "blue", "BLUE_TEAM_CHAT_CHANNEL"),
    ("FreeChatChannel", "free", "FREE_CHAT_CHANNEL"),
    (
        "SpectatorChatChannel",
        "spectator",
        "SPECTATOR_CHAT_CHANNEL",
    ),
];

fn channel_class(name: &str) -> Option<(&'static str, &'static str)> {
    CHANNEL_CLASSES
        .iter()
        .find(|(class, ..)| *class == name)
        .map(|(_, team, constant)| (*team, *constant))
}

/// `minqlx.RedTeamChatChannel()` becomes the constant `minqlx.RED_TEAM_CHAT_CHANNEL`, because the
/// four classes do not exist in shinqlx. Calls with arguments and other uses of the classes only
/// get a warning.
pub(crate) struct TeamChatChannels;

impl Rule for TeamChatChannels {
    fn name(&self) -> &'static str {
        "team-chat-channels"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        let mut rewritten_functions = HashSet::new();
        walk(source.root(), &mut |node| match node.kind() {
            "call" => {
                let Some(function) = node.child_by_field_name("function") else {
                    return;
                };
                let Some((module, (_, constant))) = module_class(function, source) else {
                    return;
                };
                let has_arguments =
                    node.child_by_field_name("arguments")
                        .is_some_and(|arguments| {
                            let mut cursor = arguments.walk();
                            arguments
                                .named_children(&mut cursor)
                                .any(|argument| argument.kind() != "comment")
                        });
                if !has_arguments {
                    output.edits.push(Edit::replace(
                        node.byte_range(),
                        format!("{module}.{constant}"),
                    ));
                    rewritten_functions.insert(function.id());
                }
            }
            "attribute" if !rewritten_functions.contains(&node.id()) => {
                if let Some((_, (team, constant))) = module_class(node, source) {
                    output
                        .warnings
                        .push(unavailable_class(line_of(node), team, constant));
                }
            }
            "identifier" => {
                let is_attribute_name = node
                    .parent()
                    .is_some_and(|parent| parent.kind() == "attribute");
                if is_attribute_name {
                    return;
                }
                if let Some((team, constant)) = channel_class(source.text_of(node)) {
                    output
                        .warnings
                        .push(unavailable_class(line_of(node), team, constant));
                }
            }
            _ => {}
        });
        output
    }
}

/// The module name and class data of an `minqlx.<ChannelClass>` or `shinqlx.<ChannelClass>` access.
fn module_class<'a>(
    node: tree_sitter::Node<'_>,
    source: &Source<'a>,
) -> Option<(&'a str, (&'static str, &'static str))> {
    if node.kind() != "attribute" {
        return None;
    }
    let module = source.text_of(node.child_by_field_name("object")?);
    let class = channel_class(source.text_of(node.child_by_field_name("attribute")?))?;
    matches!(module, "minqlx" | "shinqlx").then_some((module, class))
}

fn unavailable_class(line: usize, team: &str, constant: &str) -> Warning {
    Warning::new(
        line,
        format!(
            "this channel class does not exist in shinqlx; use the constant {constant} or \
             TeamChatChannel(\"{team}\"), and check with isinstance against TeamChatChannel"
        ),
    )
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::TeamChatChannels;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> (String, usize) {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = TeamChatChannels.apply(&source);
        let warnings = output.warnings.len();
        (
            apply_edits(text, output.edits).expect("the edits do not overlap"),
            warnings,
        )
    }

    #[rstest]
    #[case(
        "channel = minqlx.RedTeamChatChannel()\n",
        "channel = minqlx.RED_TEAM_CHAT_CHANNEL\n"
    )]
    #[case(
        "minqlx.BlueTeamChatChannel().reply('x')\n",
        "minqlx.BLUE_TEAM_CHAT_CHANNEL.reply('x')\n"
    )]
    #[case("c = shinqlx.FreeChatChannel( )\n", "c = shinqlx.FREE_CHAT_CHANNEL\n")]
    #[case(
        "c = minqlx.SpectatorChatChannel()  # all\n",
        "c = minqlx.SPECTATOR_CHAT_CHANNEL  # all\n"
    )]
    fn replaces_calls_without_arguments_by_the_constant(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(run(input), (expected.to_owned(), 0));
    }

    #[rstest]
    #[case("c = minqlx.RedTeamChatChannel(fmt='x')\n")]
    #[case("isinstance(c, minqlx.RedTeamChatChannel)\n")]
    #[case("from minqlx import RedTeamChatChannel\n")]
    fn warns_about_other_uses_and_keeps_the_text(#[case] input: &str) {
        assert_eq!(run(input), (input.to_owned(), 1));
    }

    #[rstest]
    #[case("c = minqlx.TeamChatChannel('red')\n")]
    #[case("c = other.RedTeamChatChannel()\n")]
    #[case("c = minqlx.RED_TEAM_CHAT_CHANNEL\n")]
    fn leaves_everything_else_alone(#[case] input: &str) {
        assert_eq!(run(input), (input.to_owned(), 0));
    }
}
