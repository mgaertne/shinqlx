use tree_sitter::Node;

use crate::source::Source;

/// Visits `node` and all its descendants, parents before children.
pub(crate) fn walk<'t>(node: Node<'t>, visit: &mut impl FnMut(Node<'t>)) {
    visit(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, visit);
    }
}

/// A call of the form `receiver.name(arguments)`.
pub(crate) struct MethodCall<'t> {
    pub(crate) receiver: Node<'t>,
    pub(crate) name: Node<'t>,
    pub(crate) arguments: Node<'t>,
}

pub(crate) fn method_call(node: Node<'_>) -> Option<MethodCall<'_>> {
    if node.kind() != "call" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "attribute" {
        return None;
    }
    let arguments = node.child_by_field_name("arguments")?;
    (arguments.kind() == "argument_list").then_some(())?;
    Some(MethodCall {
        receiver: function.child_by_field_name("object")?,
        name: function.child_by_field_name("attribute")?,
        arguments,
    })
}

/// The arguments of a call if all of them are plain positional expressions.
pub(crate) fn positional_arguments(arguments: Node<'_>) -> Option<Vec<Node<'_>>> {
    let mut cursor = arguments.walk();
    let expressions = arguments
        .named_children(&mut cursor)
        .filter(|argument| argument.kind() != "comment")
        .collect::<Vec<_>>();
    expressions
        .iter()
        .all(|argument| {
            !matches!(
                argument.kind(),
                "keyword_argument" | "list_splat" | "dictionary_splat"
            )
        })
        .then_some(expressions)
}

/// The name under which the file imports the shinqlx module with a plain `import`, if it does.
pub(crate) fn imported_module_name<'a>(source: &Source<'a>) -> Option<&'a str> {
    let mut imported = Vec::new();
    walk(source.root(), &mut |node| {
        if node.kind() != "import_statement" {
            return;
        }
        let mut cursor = node.walk();
        imported.extend(
            node.children_by_field_name("name", &mut cursor)
                .filter(|name| name.kind() == "dotted_name")
                .map(|name| source.text_of(name)),
        );
    });
    ["minqlx", "shinqlx"]
        .into_iter()
        .find(|module| imported.contains(module))
}

/// The argument at position `index` or given as `keyword`.
pub(crate) fn argument<'t>(
    arguments: Node<'t>,
    source: &Source<'_>,
    index: usize,
    keyword: &str,
) -> Option<Node<'t>> {
    let mut cursor = arguments.walk();
    let named = arguments
        .named_children(&mut cursor)
        .filter(|argument| argument.kind() != "comment");
    for (position, argument) in named.enumerate() {
        match argument.kind() {
            "keyword_argument" => {
                let name = argument.child_by_field_name("name")?;
                if source.text_of(name) == keyword {
                    return argument.child_by_field_name("value");
                }
            }
            "list_splat" | "dictionary_splat" => return None,
            _ if position == index => return Some(argument),
            _ => {}
        }
    }
    None
}

/// A string literal without interpolation or prefix tricks like f-strings.
pub(crate) fn is_plain_string(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    node.kind() == "string"
        && node.named_children(&mut cursor).all(|part| {
            matches!(
                part.kind(),
                "string_start" | "string_content" | "string_end"
            )
        })
}
