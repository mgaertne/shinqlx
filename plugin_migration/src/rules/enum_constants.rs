use tree_sitter::Node;

use super::{Rule, RuleOutput};
use crate::{edit::Edit, source::Source, stub::shinqlx_stub, tree::walk};

const ENUMS: [(&str, &str); 8] = [
    ("RET_", "ReturnCodes"),
    ("PRI_", "Priorities"),
    ("CVAR_", "CvarFlags"),
    ("TEAM_", "Teams"),
    ("CS_", "ConnectionStates"),
    ("MOD_", "MeansOfDeath"),
    ("PRIV_", "Privileges"),
    ("DAMAGE_", "DamageFlags"),
];

const COMPARISON_OPERATORS: [&str; 5] = ["==", "!=", "in", "not", "not in"];

/// Replaces constants like `RET_STOP` by their enum members like `ReturnCodes.STOP`, but only where
/// the member is accepted: in a `return`, a comparison and the arguments that take the enum.
pub(crate) struct EnumConstants;

pub(super) fn enum_member(constant: &str) -> Option<(&'static str, &str)> {
    let stub = shinqlx_stub();
    ENUMS.iter().find_map(|(prefix, class)| {
        let member = constant.strip_prefix(prefix)?;
        (stub.has_constant(constant) && stub.has_member(class, member)).then_some((*class, member))
    })
}

/// Moves up through parentheses, conditional branches and containers around `node`.
fn outermost_operand(node: Node<'_>) -> (Node<'_>, bool) {
    let mut current = node;
    let mut in_container = false;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "parenthesized_expression" => {}
            "conditional_expression"
                if parent
                    .named_child(1)
                    .is_none_or(|condition| condition.id() != current.id()) => {}
            "list" | "tuple" | "set" => in_container = true,
            _ => break,
        }
        current = parent;
    }
    (current, in_container)
}

fn uses_only_equality(comparison: Node<'_>, source: &Source<'_>) -> bool {
    let mut cursor = comparison.walk();
    comparison
        .children_by_field_name("operators", &mut cursor)
        .all(|operator| COMPARISON_OPERATORS.contains(&source.text_of(operator)))
}

fn callee_name<'a>(call: Node<'_>, source: &Source<'a>) -> Option<&'a str> {
    let function = call.child_by_field_name("function")?;
    let name = match function.kind() {
        "attribute" => function.child_by_field_name("attribute")?,
        _ => function,
    };
    Some(source.text_of(name))
}

fn takes_enum_as_argument(
    class: &str,
    operand: Node<'_>,
    arguments: Node<'_>,
    source: &Source<'_>,
) -> bool {
    let Some(callee) = arguments
        .parent()
        .and_then(|call| callee_name(call, source))
    else {
        return false;
    };
    let mut cursor = arguments.walk();
    let position = arguments
        .named_children(&mut cursor)
        .filter(|argument| argument.kind() != "comment")
        .position(|argument| argument.id() == operand.id());
    match (class, callee) {
        ("MeansOfDeath", "slay_with_mod") => true,
        ("Priorities", "add_hook") => position == Some(2),
        ("CvarFlags", "set_cvar" | "set_cvar_once") => position == Some(2),
        _ => false,
    }
}

fn takes_enum_as_keyword(class: &str, keyword_argument: Node<'_>, source: &Source<'_>) -> bool {
    let Some(name) = keyword_argument.child_by_field_name("name") else {
        return false;
    };
    matches!(
        (class, source.text_of(name)),
        ("Priorities", "priority")
            | ("CvarFlags", "flags")
            | ("MeansOfDeath", "mod" | "means_of_death" | "mean_of_death")
    )
}

pub(super) fn is_accepted_here(class: &str, constant: Node<'_>, source: &Source<'_>) -> bool {
    let (operand, in_container) = outermost_operand(constant);
    let Some(parent) = operand.parent() else {
        return false;
    };
    match parent.kind() {
        "comparison_operator" => uses_only_equality(parent, source),
        _ if in_container => false,
        "return_statement" => class == "ReturnCodes",
        "keyword_argument" => takes_enum_as_keyword(class, parent, source),
        "argument_list" => takes_enum_as_argument(class, operand, parent, source),
        "binary_operator" => {
            class == "CvarFlags"
                && parent
                    .child_by_field_name("operator")
                    .is_some_and(|operator| source.text_of(operator) == "|")
        }
        _ => false,
    }
}

impl Rule for EnumConstants {
    fn name(&self) -> &'static str {
        "enum-constants"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        walk(source.root(), &mut |node| {
            if node.kind() != "attribute" {
                return;
            }
            let (Some(object), Some(constant)) = (
                node.child_by_field_name("object"),
                node.child_by_field_name("attribute"),
            ) else {
                return;
            };
            if !matches!(source.text_of(object), "minqlx" | "shinqlx") {
                return;
            }
            let Some((class, member)) = enum_member(source.text_of(constant)) else {
                return;
            };
            if is_accepted_here(class, node, source) {
                output.edits.push(Edit::replace(
                    constant.byte_range(),
                    format!("{class}.{member}"),
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

    use super::{ENUMS, EnumConstants};
    use crate::{edit::apply_edits, rules::Rule, source::Source, stub::shinqlx_stub};

    fn run(text: &str) -> String {
        let source = Source::parse(text).expect("the test source is valid python");
        apply_edits(text, EnumConstants.apply(&source).edits).expect("the edits do not overlap")
    }

    #[rstest]
    #[case("return minqlx.RET_STOP_ALL\n", "return minqlx.ReturnCodes.STOP_ALL\n")]
    #[case(
        "return minqlx.RET_STOP if x else minqlx.RET_NONE\n",
        "return minqlx.ReturnCodes.STOP if x else minqlx.ReturnCodes.NONE\n"
    )]
    #[case("return (minqlx.RET_USAGE)\n", "return (minqlx.ReturnCodes.USAGE)\n")]
    #[case(
        "if res == minqlx.RET_STOP_EVENT:\n    pass\n",
        "if res == minqlx.ReturnCodes.STOP_EVENT:\n    pass\n"
    )]
    #[case(
        "ok = res != minqlx.RET_NONE\n",
        "ok = res != minqlx.ReturnCodes.NONE\n"
    )]
    #[case(
        "ok = res in [None, minqlx.RET_NONE]\n",
        "ok = res in [None, minqlx.ReturnCodes.NONE]\n"
    )]
    #[case(
        "self.add_hook('chat', self.h, priority=minqlx.PRI_LOW)\n",
        "self.add_hook('chat', self.h, priority=minqlx.Priorities.LOW)\n"
    )]
    #[case(
        "self.add_hook('chat', self.h, minqlx.PRI_HIGH)\n",
        "self.add_hook('chat', self.h, minqlx.Priorities.HIGH)\n"
    )]
    #[case(
        "self.set_cvar('a', 1, minqlx.CVAR_ARCHIVE | minqlx.CVAR_LATCH)\n",
        "self.set_cvar('a', 1, minqlx.CvarFlags.ARCHIVE | minqlx.CvarFlags.LATCH)\n"
    )]
    #[case(
        "player.slay_with_mod(minqlx.MOD_KAMIKAZE)\n",
        "player.slay_with_mod(minqlx.MeansOfDeath.KAMIKAZE)\n"
    )]
    #[case(
        "ok = player.team == minqlx.TEAM_SPECTATOR\n",
        "ok = player.team == minqlx.Teams.SPECTATOR\n"
    )]
    fn replaces_the_constant(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), expected);
    }

    #[rstest]
    #[case("x = minqlx.RET_STOP\n")]
    #[case("x = minqlx.PRI_LOW + 1\n")]
    #[case("ok = res is minqlx.RET_NONE\n")]
    #[case("ok = res < minqlx.PRI_LOW\n")]
    #[case("return [minqlx.RET_STOP]\n")]
    #[case("assert_that(code, equal_to(minqlx.RET_NONE))\n")]
    #[case("other.method(minqlx.PRI_LOW)\n")]
    #[case("d = {minqlx.RET_STOP: 1}\n")]
    #[case("return minqlx.PRI_LOW\n")]
    #[case("return minqlx.UNKNOWN_THING\n")]
    #[case("return other.RET_STOP\n")]
    fn leaves_the_constant_alone(#[case] input: &str) {
        assert_eq!(run(input), input);
    }

    #[test]
    fn every_constant_of_a_known_prefix_has_an_enum_member() {
        let stub = shinqlx_stub();
        for constant in stub.constants() {
            for (prefix, class) in ENUMS {
                if let Some(member) = constant.strip_prefix(prefix) {
                    assert!(
                        stub.has_member(class, member),
                        "{constant} has no member in {class}"
                    );
                }
            }
        }
    }
}
