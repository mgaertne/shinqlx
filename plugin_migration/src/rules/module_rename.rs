use tree_sitter::Node;

use super::{Rule, RuleOutput, Warning};
use crate::{
    edit::Edit,
    source::{Source, line_of},
};

const OLD_NAME: &str = "minqlx";
const NEW_NAME: &str = "shinqlx";

/// Renames the `minqlx` module to `shinqlx` in imports and attribute accesses.
pub(crate) struct ModuleRename;

#[derive(Debug, PartialEq, Eq)]
enum Usage {
    Rename,
    Keep,
    Rebinding,
}

fn is_field(parent: Node<'_>, field: &str, node: Node<'_>) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|child| child.id() == node.id())
}

fn classify_in_dotted_name(dotted_name: Node<'_>, node: Node<'_>) -> Usage {
    let is_first_segment = dotted_name
        .named_child(0)
        .is_some_and(|first| first.id() == node.id());
    let Some(import) = dotted_name.parent().filter(|_| is_first_segment) else {
        return Usage::Keep;
    };
    match import.kind() {
        "import_statement" | "import_from_statement" | "aliased_import" => Usage::Rename,
        _ => Usage::Keep,
    }
}

fn classify(node: Node<'_>) -> Usage {
    let Some(parent) = node.parent() else {
        return Usage::Rename;
    };
    match parent.kind() {
        "attribute" if is_field(parent, "attribute", node) => Usage::Keep,
        "keyword_argument" if is_field(parent, "name", node) => Usage::Keep,
        "function_definition" | "class_definition" | "default_parameter"
            if is_field(parent, "name", node) =>
        {
            Usage::Rebinding
        }
        "parameters" | "lambda_parameters" | "typed_parameter" => Usage::Rebinding,
        "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause"
            if is_field(parent, "left", node) =>
        {
            Usage::Rebinding
        }
        "aliased_import" if is_field(parent, "alias", node) => Usage::Rebinding,
        "dotted_name" => classify_in_dotted_name(parent, node),
        _ => Usage::Rename,
    }
}

fn collect<'t>(node: Node<'t>, source: &Source<'_>, found: &mut Vec<(Node<'t>, Usage)>) {
    if node.kind() == "identifier" && source.text_of(node) == OLD_NAME {
        found.push((node, classify(node)));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, source, found);
    }
}

impl Rule for ModuleRename {
    fn name(&self) -> &'static str {
        "module-rename"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut found = Vec::new();
        collect(source.root(), source, &mut found);

        if let Some((node, _)) = found.iter().find(|(_, usage)| *usage == Usage::Rebinding) {
            return RuleOutput {
                edits: vec![],
                warnings: vec![Warning::new(
                    line_of(*node),
                    format!("`{OLD_NAME}` is rebound in this file, the module was not renamed"),
                )],
            };
        }

        let edits = found
            .iter()
            .filter(|(_, usage)| *usage == Usage::Rename)
            .map(|(node, _)| Edit::replace(node.byte_range(), NEW_NAME))
            .collect();
        RuleOutput {
            edits,
            warnings: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::ModuleRename;
    use crate::{
        edit::apply_edits,
        rules::{Rule, RuleOutput},
        source::Source,
    };

    fn run(text: &str) -> (String, RuleOutput) {
        let source = Source::parse(text).expect("the test source is valid python");
        let mut output = ModuleRename.apply(&source);
        let edits = core::mem::take(&mut output.edits);
        let migrated = apply_edits(text, edits).expect("the edits do not overlap");
        (migrated, output)
    }

    #[rstest]
    #[case("import minqlx\n", "import shinqlx\n")]
    #[case("import os, minqlx\n", "import os, shinqlx\n")]
    #[case("import minqlx as mq\n", "import shinqlx as mq\n")]
    #[case("import minqlx.submodule\n", "import shinqlx.submodule\n")]
    #[case("from minqlx import Plugin\n", "from shinqlx import Plugin\n")]
    #[case("from minqlx.sub import Plugin\n", "from shinqlx.sub import Plugin\n")]
    #[case("from ban import ban, minqlx\n", "from ban import ban, shinqlx\n")]
    #[case("from ban import minqlx as mq\n", "from ban import shinqlx as mq\n")]
    #[case(
        "class a(minqlx.Plugin):\n    pass\n",
        "class a(shinqlx.Plugin):\n    pass\n"
    )]
    #[case("x = minqlx.RET_STOP\n", "x = shinqlx.RET_STOP\n")]
    #[case("log(minqlx)\n", "log(shinqlx)\n")]
    #[case(
        "try:\n    import minqlx\nexcept ImportError:\n    pass\n",
        "try:\n    import shinqlx\nexcept ImportError:\n    pass\n"
    )]
    fn renames_the_module(#[case] input: &str, #[case] expected: &str) {
        let (migrated, output) = run(input);
        assert_eq!(migrated, expected);
        assert!(output.warnings.is_empty());
    }

    #[rstest]
    #[case("x = other.minqlx\n")]
    #[case("f(minqlx=1)\n")]
    #[case("import pkg.minqlx\n")]
    #[case("x = 'minqlx'  # minqlx\n")]
    #[case("import os\n")]
    fn leaves_other_occurrences_alone(#[case] input: &str) {
        let (migrated, output) = run(input);
        assert_eq!(migrated, input);
        assert!(output.warnings.is_empty());
    }

    #[rstest]
    #[case("import minqlx\nminqlx = 1\n")]
    #[case("import minqlx\ndef minqlx():\n    pass\n")]
    #[case("import minqlx\ndef f(minqlx):\n    pass\n")]
    #[case("import foo as minqlx\n")]
    fn warns_instead_of_renaming_when_rebound(#[case] input: &str) {
        let (migrated, output) = run(input);
        assert_eq!(migrated, input);
        assert_eq!(output.warnings.len(), 1);
    }
}
