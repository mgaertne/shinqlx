use super::{Rule, RuleOutput, Warning};
use crate::{
    source::{Source, line_of},
    tree::{method_call, walk},
};

/// Handlers that a plugin adds to the logger of shinqlx, and `main_logger`, which the module of
/// shinqlx does not have. Both only get a warning, as the right replacement depends on the plugin.
pub(crate) struct LoggerHandlers;

impl Rule for LoggerHandlers {
    fn name(&self) -> &'static str {
        "logger-handlers"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        let uses_logger = uses_shinqlx_logger(source);
        walk(source.root(), &mut |node| {
            if node.kind() == "attribute" && is_main_logger(node, source) {
                output.warnings.push(Warning::new(
                    line_of(node),
                    "main_logger is not part of shinqlx, use get_logger()",
                ));
                return;
            }
            let Some(call) = method_call(node) else {
                return;
            };
            if uses_logger && matches!(source.text_of(call.name), "addHandler" | "removeHandler") {
                output.warnings.push(Warning::new(
                    line_of(node),
                    "shinqlx writes its logs through shinqlx_log.yml: a handler on the logger of shinqlx \
                     still gets the messages of plugins, but not the debug records of the dispatchers and \
                     the console lines. Configure files and levels in shinqlx_log.yml instead",
                ));
            }
        });
        output
    }
}

fn is_main_logger(node: tree_sitter::Node<'_>, source: &Source<'_>) -> bool {
    let module = node
        .child_by_field_name("object")
        .map(|object| source.text_of(object));
    let attribute = node
        .child_by_field_name("attribute")
        .map(|attribute| source.text_of(attribute));
    matches!(module, Some("minqlx" | "shinqlx")) && attribute == Some("main_logger")
}

/// Whether the file gets a logger of shinqlx at all, so that `addHandler` of an unrelated object
/// does not produce a warning.
fn uses_shinqlx_logger(source: &Source<'_>) -> bool {
    let mut found = false;
    walk(source.root(), &mut |node| {
        found = found || names_shinqlx_logger(node, source);
    });
    found
}

fn names_shinqlx_logger(node: tree_sitter::Node<'_>, source: &Source<'_>) -> bool {
    let field_text = |field| {
        node.child_by_field_name(field)
            .map(|child| source.text_of(child))
    };
    match node.kind() {
        "identifier" => source.text_of(node) == "get_logger",
        "attribute" => matches!(field_text("attribute"), Some("logger" | "main_logger")),
        "call" => {
            field_text("function").is_some_and(|function| function.ends_with("getLogger"))
                && field_text("arguments").is_some_and(|arguments| {
                    arguments.contains("minqlx") || arguments.contains("shinqlx")
                })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::LoggerHandlers;
    use crate::{rules::Rule, source::Source};

    fn warnings(text: &str) -> usize {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = LoggerHandlers.apply(&source);
        assert!(output.edits.is_empty());
        output.warnings.len()
    }

    #[rstest]
    #[case("logger = minqlx.get_logger()\nlogger.addHandler(handler)\n", 1)]
    #[case("self.logger.removeHandler(handler)\n", 1)]
    #[case("logging.getLogger('minqlx').addHandler(h)\n", 1)]
    #[case("log = minqlx.main_logger\n", 1)]
    #[case("minqlx.main_logger.addHandler(h)\n", 2)]
    fn warns_about_handlers_of_the_shinqlx_logger(#[case] input: &str, #[case] expected: usize) {
        assert_eq!(warnings(input), expected);
    }

    #[rstest]
    #[case("server.addHandler(callback)\n")]
    #[case("logging.getLogger('other').addHandler(h)\n")]
    #[case("logger = minqlx.get_logger()\nlogger.info('x')\n")]
    fn leaves_everything_else_alone(#[case] input: &str) {
        assert_eq!(warnings(input), 0);
    }
}
