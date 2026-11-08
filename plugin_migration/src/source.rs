use tree_sitter::{Node, Parser, Tree};

/// A parsed python source file.
pub(crate) struct Source<'a> {
    pub(crate) text: &'a str,
    tree: Tree,
}

impl<'a> Source<'a> {
    /// Parses `text`; returns `None` if it is not understood completely.
    pub(crate) fn parse(text: &'a str) -> Option<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("the bundled python grammar matches the tree-sitter ABI");
        let tree = parser.parse(text, None)?;
        (!tree.root_node().has_error()).then_some(Self { text, tree })
    }

    pub(crate) fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub(crate) fn text_of(&self, node: Node<'_>) -> &'a str {
        &self.text[node.byte_range()]
    }
}

pub(crate) fn line_of(node: Node<'_>) -> usize {
    node.start_position().row + 1
}

#[cfg(test)]
mod tests {
    use super::Source;

    #[test]
    fn parses_valid_python() {
        assert!(Source::parse("import minqlx\n").is_some());
    }

    #[test]
    fn rejects_syntax_errors() {
        assert!(Source::parse("def broken(:\n").is_none());
    }
}
