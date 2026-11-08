use std::collections::HashMap;

use tree_sitter::Node;

use super::{Rule, RuleOutput, Warning};
use crate::{
    edit::Edit,
    source::{Source, line_of},
    stub::shinqlx_stub,
    tree::{argument, imported_module_name, is_plain_string, method_call, walk},
};

const HOOK_ARGUMENTS: [&str; 3] = ["event", "handler", "priority"];
const COMMAND_ARGUMENTS: [&str; 10] = [
    "name",
    "handler",
    "permission",
    "channels",
    "exclude_channels",
    "priority",
    "client_cmd_pass",
    "client_cmd_perm",
    "prefix",
    "usage",
];

/// Moves `self.add_hook(...)` and `self.add_command(...)` calls from `__init__` to `hook` and
/// `command` decorators on the handler methods.
pub(crate) struct Decorators;

/// A registration call that becomes a decorator of `method`.
struct Conversion<'t> {
    statement: Node<'t>,
    method: &'t str,
    decorator: String,
}

struct Context<'a, 't> {
    source: &'a Source<'t>,
    module: &'a str,
    methods: &'a HashMap<&'t str, Node<'t>>,
}

fn is_static(node: Node<'_>, source: &Source<'_>, module: &str) -> bool {
    let all_static = |node: Node<'_>| {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .filter(|child| child.kind() != "comment")
            .all(|child| is_static(child, source, module))
    };
    match node.kind() {
        "string" => is_plain_string(node),
        "integer" | "float" | "true" | "false" | "none" => true,
        "unary_operator" => node
            .child_by_field_name("argument")
            .is_some_and(|operand| is_static(operand, source, module)),
        "tuple" | "list" | "parenthesized_expression" => all_static(node),
        "binary_operator" => {
            node.child_by_field_name("operator")
                .is_some_and(|operator| source.text_of(operator) == "|")
                && ["left", "right"].iter().all(|field| {
                    node.child_by_field_name(field)
                        .is_some_and(|operand| is_static(operand, source, module))
                })
        }
        "attribute" => is_module_member(node, source, module),
        _ => false,
    }
}

/// `module.CONSTANT` or `module.Class.MEMBER` of the shinqlx module.
fn is_module_member(node: Node<'_>, source: &Source<'_>, module: &str) -> bool {
    let stub = shinqlx_stub();
    let (Some(object), Some(name)) = (
        node.child_by_field_name("object"),
        node.child_by_field_name("attribute"),
    ) else {
        return false;
    };
    match object.kind() {
        "identifier" => {
            source.text_of(object) == module
                && (stub.has_constant(source.text_of(name)) || stub.has_class(source.text_of(name)))
        }
        "attribute" => {
            let class = object.child_by_field_name("attribute");
            let root = object.child_by_field_name("object");
            matches!((root, class), (Some(root), Some(class))
                if root.kind() == "identifier"
                    && source.text_of(root) == module
                    && stub.has_member(source.text_of(class), source.text_of(name)))
        }
        _ => false,
    }
}

fn option_text(
    arguments: Node<'_>,
    context: &Context<'_, '_>,
    name: &str,
    index: usize,
) -> Result<Option<String>, String> {
    let Some(value) = argument(arguments, context.source, index, name) else {
        return Ok(None);
    };
    let text = context.source.text_of(value);
    if matches!(name, "channels" | "exclude_channels") {
        return match text {
            "None" | "()" | "[]" => Ok(None),
            _ => Err(format!(
                "the argument {name} is not supported by the decorator"
            )),
        };
    }
    if !is_static(value, context.source, context.module) {
        return Err(format!("the argument {name} is not a constant"));
    }
    Ok(Some(format!("{name}={text}")))
}

fn unknown_arguments(arguments: Node<'_>, context: &Context<'_, '_>, known: &[&str]) -> bool {
    let mut cursor = arguments.walk();
    let mut position = 0;
    for argument in arguments.named_children(&mut cursor) {
        match argument.kind() {
            "comment" => {}
            "keyword_argument" => {
                let known_keyword = argument
                    .child_by_field_name("name")
                    .is_some_and(|name| known.contains(&context.source.text_of(name)));
                if !known_keyword {
                    return true;
                }
            }
            "list_splat" | "dictionary_splat" => return true,
            _ => {
                if position >= known.len() {
                    return true;
                }
                position += 1;
            }
        }
    }
    false
}

/// `thread`, `next_frame` and `delay` wrap the method in a `DeferredCall`, which the decorators accept.
fn only_deferrals(decorated: Node<'_>, context: &Context<'_, '_>) -> bool {
    let module = context.module;
    let mut cursor = decorated.walk();
    decorated
        .children(&mut cursor)
        .filter(|child| child.kind() == "decorator")
        .all(|decorator| {
            let text = context
                .source
                .text_of(decorator)
                .trim_start_matches('@')
                .trim();
            text == format!("{module}.thread")
                || text == format!("{module}.next_frame")
                || text.starts_with(&format!("{module}.delay("))
        })
}

fn handler_method<'t>(arguments: Node<'t>, context: &Context<'_, 't>) -> Result<&'t str, String> {
    let handler =
        argument(arguments, context.source, 1, "handler").ok_or("the handler is missing")?;
    let is_self_attribute = handler.kind() == "attribute"
        && handler
            .child_by_field_name("object")
            .is_some_and(|object| context.source.text_of(object) == "self");
    let name = handler
        .child_by_field_name("attribute")
        .filter(|_| is_self_attribute)
        .map(|name| context.source.text_of(name))
        .ok_or("the handler is not a method of this class")?;
    match context.methods.get(name) {
        Some(method)
            if method.kind() == "function_definition" || only_deferrals(*method, context) =>
        {
            Ok(name)
        }
        Some(_) => Err(format!("the handler {name} has other decorators")),
        None => Err(format!("the handler {name} is not defined in this class")),
    }
}

fn hook_decorator(arguments: Node<'_>, context: &Context<'_, '_>) -> Result<String, String> {
    if unknown_arguments(arguments, context, &HOOK_ARGUMENTS) {
        return Err("the arguments are not understood".to_string());
    }
    let event = argument(arguments, context.source, 0, "event").ok_or("the event is missing")?;
    if !is_static(event, context.source, context.module) {
        return Err("the event is not a constant".to_string());
    }
    let mut parts = vec![context.source.text_of(event).to_owned()];
    parts.extend(option_text(arguments, context, "priority", 2)?);
    Ok(format!("@{}.hook({})", context.module, parts.join(", ")))
}

fn command_decorator(arguments: Node<'_>, context: &Context<'_, '_>) -> Result<String, String> {
    if unknown_arguments(arguments, context, &COMMAND_ARGUMENTS) {
        return Err("the arguments are not understood".to_string());
    }
    let name = argument(arguments, context.source, 0, "name").ok_or("the name is missing")?;
    if !is_static(name, context.source, context.module) {
        return Err("the name of the command is not a constant".to_string());
    }
    let mut parts = vec![context.source.text_of(name).to_owned()];
    for (offset, option) in COMMAND_ARGUMENTS[2..].iter().enumerate() {
        parts.extend(option_text(arguments, context, option, offset + 2)?);
    }
    Ok(format!("@{}.command({})", context.module, parts.join(", ")))
}

fn convert<'t>(
    statement: Node<'t>,
    context: &Context<'_, 't>,
) -> Option<Result<Conversion<'t>, String>> {
    let call = method_call(
        statement
            .named_child(0)
            .filter(|_| statement.named_child_count() == 1)?,
    )?;
    if context.source.text_of(call.receiver) != "self" {
        return None;
    }
    let kind = context.source.text_of(call.name);
    if !matches!(kind, "add_hook" | "add_command") {
        return None;
    }
    let result = handler_method(call.arguments, context).and_then(|method| {
        let decorator = if kind == "add_hook" {
            hook_decorator(call.arguments, context)?
        } else {
            command_decorator(call.arguments, context)?
        };
        Ok(Conversion {
            statement,
            method,
            decorator,
        })
    });
    Some(result)
}

/// The byte range of the whole lines of `node`, if nothing else but whitespace and a comment is on them.
fn whole_lines(node: Node<'_>, text: &str) -> Option<core::ops::Range<usize>> {
    let start = text[..node.start_byte()]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let end = text[node.end_byte()..]
        .find('\n')
        .map_or(text.len(), |index| node.end_byte() + index + 1);
    let before = &text[start..node.start_byte()];
    let after = text[node.end_byte()..end].trim();
    (before.trim().is_empty() && (after.is_empty() || after.starts_with('#'))).then_some(start..end)
}

fn class_methods<'t>(body: Node<'t>, source: &Source<'t>) -> HashMap<&'t str, Node<'t>> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|statement| {
            let definition = match statement.kind() {
                "function_definition" => statement,
                "decorated_definition" => statement.child_by_field_name("definition")?,
                _ => return None,
            };
            let name = source.text_of(definition.child_by_field_name("name")?);
            Some((name, statement))
        })
        .collect()
}

fn registers_dynamically(class: Node<'_>, source: &Source<'_>) -> bool {
    let mut found = false;
    walk(class, &mut |node| {
        found |= method_call(node).is_some_and(|call| {
            matches!(source.text_of(call.name), "remove_hook" | "remove_command")
        });
    });
    found
}

fn convert_class(class: Node<'_>, source: &Source<'_>, module: &str, output: &mut RuleOutput) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let methods = class_methods(body, source);
    let Some((init_definition, init)) = methods
        .get("__init__")
        .filter(|init| init.kind() == "function_definition")
        .and_then(|definition| Some((*definition, definition.child_by_field_name("body")?)))
    else {
        return;
    };
    let context = Context {
        source,
        module,
        methods: &methods,
    };

    let mut cursor = init.walk();
    let statements = init
        .named_children(&mut cursor)
        .filter(|statement| statement.kind() != "comment")
        .collect::<Vec<_>>();
    let mut conversions = Vec::new();
    let mut remaining = 0;
    let mut kept = 0;
    for statement in &statements {
        match convert(*statement, &context) {
            Some(Ok(conversion)) => conversions.push(conversion),
            Some(Err(reason)) => {
                remaining += 1;
                kept += 1;
                output.warnings.push(Warning::new(
                    line_of(*statement),
                    format!("the registration was kept as it is: {reason}"),
                ));
            }
            None => remaining += 1,
        }
    }
    if conversions.is_empty() {
        return;
    }
    if registers_dynamically(class, source) {
        output.warnings.push(Warning::new(
            line_of(class),
            "the class removes hooks or commands, the registrations were kept as they are",
        ));
        return;
    }

    let mut edits = Vec::new();
    if remaining == 0 {
        let Some(lines) = whole_lines(init_definition, source.text) else {
            return;
        };
        edits.push(Edit::replace(lines, ""));
    }
    for conversion in &conversions {
        if remaining > 0 {
            let Some(lines) = whole_lines(conversion.statement, source.text) else {
                output.warnings.push(Warning::new(
                    line_of(conversion.statement),
                    "the registration shares its line with other code and was kept as it is",
                ));
                return;
            };
            edits.push(Edit::replace(lines, ""));
        }

        let Some(definition) = methods.get(conversion.method) else {
            return;
        };
        let line_start = source.text[..definition.start_byte()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let indent = &source.text[line_start..definition.start_byte()];
        edits.push(Edit::replace(
            line_start..line_start,
            format!("{indent}{}\n", conversion.decorator),
        ));
    }
    if kept > 0 {
        output.warnings.push(Warning::new(
            line_of(class),
            "hooks and commands are registered both manually and by decorators, handlers of the \
             same priority may run in a different order",
        ));
    }
    output.edits.extend(edits);
}

impl Rule for Decorators {
    fn name(&self) -> &'static str {
        "decorators"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        let Some(module) = imported_module_name(source) else {
            return output;
        };
        walk(source.root(), &mut |node| {
            if node.kind() == "class_definition" {
                convert_class(node, source, module, &mut output);
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::Decorators;
    use crate::{edit::apply_edits, rules::Rule, source::Source};

    fn run(text: &str) -> (String, usize) {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = Decorators.apply(&source);
        let warnings = output.warnings.len();
        (
            apply_edits(text, output.edits).expect("the edits do not overlap"),
            warnings,
        )
    }

    #[test]
    fn converts_hooks_and_commands() {
        let input = "\
import minqlx

class greeter(minqlx.Plugin):
    def __init__(self):
        super().__init__()
        self.add_hook(\"player_connect\", self.handle_connect, priority=minqlx.PRI_LOW)
        self.add_command(\"hello\", self.cmd_hello, 1, usage=\"<name>\")
        self.add_command((\"a\", \"b\"), self.cmd_hello)

    def handle_connect(self, player):
        pass

    def cmd_hello(self, player, msg, channel):
        pass
";
        let expected = "\
import minqlx

class greeter(minqlx.Plugin):
    def __init__(self):
        super().__init__()

    @minqlx.hook(\"player_connect\", priority=minqlx.PRI_LOW)
    def handle_connect(self, player):
        pass

    @minqlx.command(\"hello\", permission=1, usage=\"<name>\")
    @minqlx.command((\"a\", \"b\"))
    def cmd_hello(self, player, msg, channel):
        pass
";
        assert_eq!(run(input), (expected.to_string(), 0));
    }

    #[test]
    fn keeps_registrations_that_cannot_be_converted() {
        let input = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        super().__init__()
        self.add_hook(\"chat\", self.handle_chat)
        self.add_command(\"x\", self.cmd_x, self.get_cvar(\"perm\", int))
        self.add_command(\"y\", self.cmd_y, channels=self.some_channels)
        self.add_command(\"z\", self.inherited)
        if self.get_cvar(\"extra\"):
            self.add_command(\"w\", self.cmd_w)

    def handle_chat(self, player, msg, channel):
        pass

    def cmd_x(self, player, msg, channel):
        pass

    def cmd_y(self, player, msg, channel):
        pass

    def cmd_w(self, player, msg, channel):
        pass
";
        let (output, warnings) = run(input);
        assert!(output.contains("@minqlx.hook(\"chat\")\n    def handle_chat"));
        assert!(
            output.contains("self.add_command(\"x\", self.cmd_x, self.get_cvar(\"perm\", int))")
        );
        assert!(
            output.contains("self.add_command(\"y\", self.cmd_y, channels=self.some_channels)")
        );
        assert!(output.contains("self.add_command(\"z\", self.inherited)"));
        assert!(output.contains("self.add_command(\"w\", self.cmd_w)"));
        assert_eq!(warnings, 4);
    }

    #[test]
    fn leaves_classes_that_remove_registrations() {
        let input = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        super().__init__()
        self.add_hook(\"chat\", self.handle_chat)

    def handle_chat(self, player, msg, channel):
        pass

    def unload(self):
        self.remove_hook(\"chat\", self.handle_chat)
";
        assert_eq!(run(input), (input.to_string(), 1));
    }

    #[test]
    fn removes_an_init_that_only_registered() {
        let input = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        self.add_hook(\"chat\", self.handle_chat)

    def handle_chat(self, player, msg, channel):
        pass
";
        let expected = "\
import minqlx

class p(minqlx.Plugin):

    @minqlx.hook(\"chat\")
    def handle_chat(self, player, msg, channel):
        pass
";
        assert_eq!(run(input), (expected.to_string(), 0));
    }

    #[test]
    fn puts_the_decorator_above_deferrals() {
        let input = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        super().__init__()
        self.add_command(\"x\", self.cmd_x)

    @minqlx.thread
    def cmd_x(self, player, msg, channel):
        pass
";
        let expected = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        super().__init__()

    @minqlx.command(\"x\")
    @minqlx.thread
    def cmd_x(self, player, msg, channel):
        pass
";
        assert_eq!(run(input), (expected.to_string(), 0));
    }

    #[test]
    fn leaves_handlers_with_other_decorators_alone() {
        let input = "\
import minqlx

class p(minqlx.Plugin):
    def __init__(self):
        super().__init__()
        self.add_command(\"x\", self.cmd_x)

    @some.decorator
    def cmd_x(self, player, msg, channel):
        pass
";
        assert_eq!(run(input), (input.to_string(), 1));
    }
}
