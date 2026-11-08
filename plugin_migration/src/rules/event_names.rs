use super::{Rule, RuleOutput};
use crate::{
    edit::Edit,
    source::Source,
    stub::shinqlx_stub,
    tree::{argument, imported_module_name, is_plain_string, method_call, walk},
};

/// Replaces the event name strings in `add_hook` and `remove_hook` by the members of `Events`.
pub(crate) struct EventNames;

impl Rule for EventNames {
    fn name(&self) -> &'static str {
        "event-names"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        let Some(module) = imported_module_name(source) else {
            return output;
        };
        let stub = shinqlx_stub();

        walk(source.root(), &mut |node| {
            let Some(call) = method_call(node) else {
                return;
            };
            if !matches!(source.text_of(call.name), "add_hook" | "remove_hook") {
                return;
            }
            let Some(event) =
                argument(call.arguments, source, 0, "event").filter(|e| is_plain_string(*e))
            else {
                return;
            };
            let name = source.text_of(event).trim_matches(['"', '\'']);
            if let Some(member) = stub.member_with_value("Events", name) {
                output.edits.push(Edit::replace(
                    event.byte_range(),
                    format!("{module}.Events.{member}"),
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

    use super::EventNames;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> String {
        let source = Source::parse(text).expect("the test source is valid python");
        apply_edits(text, EventNames.apply(&source).edits).expect("the edits do not overlap")
    }

    #[rstest]
    #[case(
        "import minqlx\nself.add_hook(\"player_connect\", self.h)\n",
        "import minqlx\nself.add_hook(minqlx.Events.PLAYER_CONNECT, self.h)\n"
    )]
    #[case(
        "import minqlx\nself.add_hook('chat', self.h, priority=minqlx.PRI_LOW)\n",
        "import minqlx\nself.add_hook(minqlx.Events.CHAT, self.h, priority=minqlx.PRI_LOW)\n"
    )]
    #[case(
        "import minqlx\nself.remove_hook('chat', self.h)\n",
        "import minqlx\nself.remove_hook(minqlx.Events.CHAT, self.h)\n"
    )]
    #[case(
        "import minqlx\nself.add_hook(event='chat', handler=self.h)\n",
        "import minqlx\nself.add_hook(event=minqlx.Events.CHAT, handler=self.h)\n"
    )]
    #[case(
        "import shinqlx\nself.add_hook('chat', self.h)\n",
        "import shinqlx\nself.add_hook(shinqlx.Events.CHAT, self.h)\n"
    )]
    fn replaces_the_event_name(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), expected);
    }

    #[rstest]
    #[case("import minqlx\nself.add_hook('my_own_event', self.h)\n")]
    #[case("import minqlx\nself.add_hook(name, self.h)\n")]
    #[case("import minqlx\nself.add_hook(f'chat{x}', self.h)\n")]
    #[case("import minqlx\nself.other('chat', self.h)\n")]
    #[case("from minqlx import Plugin\nself.add_hook('chat', self.h)\n")]
    #[case("import minqlx as mq\nself.add_hook('chat', self.h)\n")]
    fn leaves_other_calls_alone(#[case] input: &str) {
        assert_eq!(run(input), input);
    }
}
