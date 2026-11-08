use super::{Rule, RuleOutput};
use crate::{
    edit::Edit,
    source::Source,
    tree::{method_call, walk},
};

/// `Player.slay_with_mod` calls its parameter `means_of_death`, the module function `mean_of_death`,
/// both not `mod`, which is a reserved word in Rust.
pub(crate) struct SlayWithMod;

impl Rule for SlayWithMod {
    fn name(&self) -> &'static str {
        "slay-with-mod"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        walk(source.root(), &mut |node| {
            let Some(call) = method_call(node) else {
                return;
            };
            if source.text_of(call.name) != "slay_with_mod" {
                return;
            }
            let replacement = match source.text_of(call.receiver) {
                "minqlx" | "shinqlx" => "mean_of_death",
                _ => "means_of_death",
            };
            let mut cursor = call.arguments.walk();
            for argument in call.arguments.named_children(&mut cursor) {
                let keyword = argument
                    .child_by_field_name("name")
                    .filter(|_| argument.kind() == "keyword_argument")
                    .filter(|name| source.text_of(*name) == "mod");
                if let Some(keyword) = keyword {
                    output
                        .edits
                        .push(Edit::replace(keyword.byte_range(), replacement));
                }
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::SlayWithMod;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> String {
        let source = Source::parse(text).expect("the test source is valid python");
        apply_edits(text, SlayWithMod.apply(&source).edits).expect("the edits do not overlap")
    }

    #[rstest]
    #[case(
        "player.slay_with_mod(mod=minqlx.MOD_KAMIKAZE)\n",
        "player.slay_with_mod(means_of_death=minqlx.MOD_KAMIKAZE)\n"
    )]
    #[case(
        "minqlx.slay_with_mod(1, mod=3)\n",
        "minqlx.slay_with_mod(1, mean_of_death=3)\n"
    )]
    #[case("player.slay_with_mod(3)\n", "player.slay_with_mod(3)\n")]
    #[case("player.other(mod=3)\n", "player.other(mod=3)\n")]
    fn renames_the_keyword(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), expected);
    }
}
