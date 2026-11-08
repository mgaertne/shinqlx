use tree_sitter::Node;

use super::{
    Rule, RuleOutput, Warning,
    enum_constants::{enum_member, is_accepted_here},
};
use crate::{
    edit::Edit,
    source::{Source, line_of},
    tree::walk,
};

/// Replaces constants imported with `from minqlx import RET_STOP` by the members of their enum and
/// imports the enum instead, but only where the member is accepted, see [`super::EnumConstants`].
pub(crate) struct EnumImports;

fn is_binding(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let is_field = |field| {
        parent
            .child_by_field_name(field)
            .is_some_and(|child| child.id() == node.id())
    };
    match parent.kind() {
        "parameters" | "global_statement" | "nonlocal_statement" | "pattern_list"
        | "tuple_pattern" | "list_pattern" | "list_splat_pattern" | "typed_parameter"
        | "as_pattern_target" => true,
        "default_parameter"
        | "typed_default_parameter"
        | "function_definition"
        | "class_definition"
        | "named_expression" => is_field("name"),
        "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause" => {
            is_field("left")
        }
        _ => false,
    }
}

fn is_name_in_other_expression(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let is_field = |field| {
        parent
            .child_by_field_name(field)
            .is_some_and(|child| child.id() == node.id())
    };
    match parent.kind() {
        "attribute" => is_field("attribute"),
        "keyword_argument" => is_field("name"),
        _ => false,
    }
}

fn is_inside_import(node: Node<'_>) -> bool {
    std::iter::successors(node.parent(), |parent| parent.parent()).any(|parent| {
        matches!(
            parent.kind(),
            "import_statement" | "import_from_statement" | "future_import_statement"
        )
    })
}

fn uses_of<'t>(name: &str, source: &'t Source<'_>) -> Vec<Node<'t>> {
    let mut uses = Vec::new();
    walk(source.root(), &mut |node| {
        if node.kind() == "identifier"
            && source.text_of(node) == name
            && !is_name_in_other_expression(node)
            && !is_inside_import(node)
        {
            uses.push(node);
        }
    });
    uses
}

fn is_shinqlx_import(statement: Node<'_>, source: &Source<'_>) -> bool {
    statement.kind() == "import_from_statement"
        && statement
            .child_by_field_name("module_name")
            .is_some_and(|module| matches!(source.text_of(module), "minqlx" | "shinqlx"))
}

fn is_otherwise_bound(class: &str, source: &Source<'_>) -> bool {
    uses_of_everywhere(class, source).into_iter().any(|node| {
        is_binding(node)
            || std::iter::successors(node.parent(), |parent| parent.parent())
                .find(|parent| {
                    matches!(
                        parent.kind(),
                        "import_statement" | "import_from_statement" | "future_import_statement"
                    )
                })
                .is_some_and(|import| !is_shinqlx_import(import, source))
    })
}

fn uses_of_everywhere<'t>(name: &str, source: &'t Source<'_>) -> Vec<Node<'t>> {
    let mut found = Vec::new();
    walk(source.root(), &mut |node| {
        if node.kind() == "identifier" && source.text_of(node) == name {
            found.push(node);
        }
    });
    found
}

struct Conversion<'t> {
    constant: &'t str,
    class: &'static str,
    member: &'t str,
    uses: Vec<Node<'t>>,
    all_converted: bool,
}

fn conversion_of<'t>(constant: &'t str, source: &'t Source<'_>) -> Option<Conversion<'t>> {
    let (class, member) = enum_member(constant)?;
    if is_otherwise_bound(class, source) {
        return None;
    }
    let uses = uses_of(constant, source);
    if uses.iter().any(|node| is_binding(*node)) {
        return None;
    }
    let (accepted, others): (Vec<_>, Vec<_>) = uses
        .into_iter()
        .partition(|node| is_accepted_here(class, *node, source));
    (!accepted.is_empty()).then_some(Conversion {
        constant,
        class,
        member,
        uses: accepted,
        all_converted: others.is_empty(),
    })
}

fn convert_import(statement: Node<'_>, source: &Source<'_>, output: &mut RuleOutput) {
    let mut cursor = statement.walk();
    let names = statement
        .children_by_field_name("name", &mut cursor)
        .collect::<Vec<_>>();
    let conversions = names
        .iter()
        .filter(|name| name.kind() == "dotted_name")
        .filter_map(|name| conversion_of(source.text_of(*name), source))
        .collect::<Vec<_>>();
    let (Some(first), Some(last)) = (names.first(), names.last()) else {
        return;
    };
    if conversions.is_empty() {
        return;
    }
    if statement.start_position().row != statement.end_position().row {
        output.warnings.push(Warning::new(
            line_of(statement),
            "the import spans several lines, its constants were not replaced by enumerations",
        ));
        return;
    }

    let mut kept = names
        .iter()
        .map(|name| source.text_of(*name))
        .filter(|name| {
            !conversions
                .iter()
                .any(|conversion| conversion.all_converted && conversion.constant == *name)
        })
        .collect::<Vec<_>>();
    for conversion in &conversions {
        if !kept.contains(&conversion.class) {
            kept.push(conversion.class);
        }
        for node in &conversion.uses {
            output.edits.push(Edit::replace(
                node.byte_range(),
                format!("{}.{}", conversion.class, conversion.member),
            ));
        }
    }
    output.edits.push(Edit::replace(
        first.start_byte()..last.end_byte(),
        kept.join(", "),
    ));
}

impl Rule for EnumImports {
    fn name(&self) -> &'static str {
        "enum-imports"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        walk(source.root(), &mut |node| {
            if is_shinqlx_import(node, source) {
                convert_import(node, source, &mut output);
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::EnumImports;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> (String, usize) {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = EnumImports.apply(&source);
        let warnings = output.warnings.len();
        let text = apply_edits(text, output.edits).expect("the edits do not overlap");
        (text, warnings)
    }

    #[rstest]
    #[case(
        "from minqlx import RET_STOP, Plugin\n\ndef f():\n    return RET_STOP\n",
        "from minqlx import Plugin, ReturnCodes\n\ndef f():\n    return ReturnCodes.STOP\n"
    )]
    #[case(
        "from minqlx import RET_STOP, RET_USAGE\n\ndef f(x):\n    if x:\n        return RET_USAGE\n    return RET_STOP\n",
        "from minqlx import ReturnCodes\n\ndef f(x):\n    if x:\n        return ReturnCodes.USAGE\n    return ReturnCodes.STOP\n"
    )]
    #[case(
        "from shinqlx import RET_STOP, ReturnCodes\n\ndef f():\n    return RET_STOP\n",
        "from shinqlx import ReturnCodes\n\ndef f():\n    return ReturnCodes.STOP\n"
    )]
    #[case(
        "from minqlx import (RET_STOP_ALL, Plugin)\n\ndef f(res):\n    return res == RET_STOP_ALL\n",
        "from minqlx import (Plugin, ReturnCodes)\n\ndef f(res):\n    return res == ReturnCodes.STOP_ALL\n"
    )]
    #[case(
        "from minqlx import PRI_LOW\n\nself.add_hook('chat', self.h, priority=PRI_LOW)\n",
        "from minqlx import Priorities\n\nself.add_hook('chat', self.h, priority=Priorities.LOW)\n"
    )]
    #[case(
        "from minqlx import RET_STOP\n\ndef f():\n    return RET_STOP\n\nCODE = RET_STOP\n",
        "from minqlx import RET_STOP, ReturnCodes\n\ndef f():\n    return ReturnCodes.STOP\n\nCODE = RET_STOP\n"
    )]
    fn replaces_the_imported_constant(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(run(input), (expected.to_string(), 0));
    }

    #[rstest]
    #[case("from minqlx import RET_STOP as STOP\n\ndef f():\n    return STOP\n")]
    #[case("from minqlx import *\n\ndef f():\n    return RET_STOP\n")]
    #[case("from other import RET_STOP\n\ndef f():\n    return RET_STOP\n")]
    #[case("from minqlx import RET_STOP\n\nCODE = RET_STOP\n")]
    #[case("from minqlx import RET_STOP\n\ndef f(RET_STOP):\n    return RET_STOP\n")]
    #[case("from minqlx import RET_STOP\n\nRET_STOP = 1\n\ndef f():\n    return RET_STOP\n")]
    #[case("from minqlx import RET_STOP\n\ndef f(o):\n    return o.RET_STOP\n")]
    #[case("from minqlx import Plugin, Player\n")]
    #[case(
        "from minqlx import RET_STOP\nfrom other import ReturnCodes\n\ndef f():\n    return RET_STOP\n"
    )]
    #[case(
        "from minqlx import RET_STOP\n\nclass ReturnCodes:\n    pass\n\ndef f():\n    return RET_STOP\n"
    )]
    #[case("from minqlx import RET_STOP\n\nReturnCodes = {}\n\ndef f():\n    return RET_STOP\n")]
    fn leaves_the_import_alone(#[case] input: &str) {
        assert_eq!(run(input), (input.to_string(), 0));
    }

    #[test]
    fn warns_about_an_import_over_several_lines() {
        let input = "from minqlx import (\n    RET_STOP,\n    Plugin,\n)\n\ndef f():\n    return RET_STOP\n";
        assert_eq!(run(input), (input.to_string(), 1));
    }
}
