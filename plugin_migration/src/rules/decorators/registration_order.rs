//! Handlers of the same event and priority run in the order they were registered. The loader registers the decorated
//! methods after `__init__`, in the alphabetical order of their names, so converting a registration can change that
//! order. These checks find the places where it can.

use tree_sitter::Node;

use super::Conversion;
use crate::{
    rules::Warning,
    source::{Source, line_of},
    tree::{argument, is_plain_string, method_call, walk},
};

const PRIORITIES: [&str; 5] = ["lowest", "low", "normal", "high", "highest"];

/// What a call of `add_hook` or `add_command` registers, as far as its text tells. An unknown part matches anything.
#[derive(Debug)]
pub(super) struct Registration {
    is_hook: bool,
    names: Option<Vec<String>>,
    priority: Option<&'static str>,
}

/// The names an event or command argument stands for: a string, the member of an enumeration (`Events.CHAT` is
/// `chat`) or a collection of those. `None` if any of them is not known from the text.
fn names_of(node: Node<'_>, source: &Source<'_>) -> Option<Vec<String>> {
    match node.kind() {
        "string" if is_plain_string(node) => Some(vec![
            source
                .text_of(node)
                .trim_matches(['"', '\''])
                .to_lowercase(),
        ]),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|member| vec![source.text_of(member).to_lowercase()]),
        "tuple" | "list" | "parenthesized_expression" => {
            let mut cursor = node.walk();
            let parts = node
                .named_children(&mut cursor)
                .filter(|child| child.kind() != "comment")
                .map(|child| names_of(child, source))
                .collect::<Option<Vec<_>>>()?;
            Some(parts.concat())
        }
        _ => None,
    }
}

fn priority_name(text: &str) -> Option<&'static str> {
    let last = text.rsplit('.').next().unwrap_or(text).to_lowercase();
    let name = last.strip_prefix("pri_").unwrap_or(&last);
    PRIORITIES.into_iter().find(|priority| *priority == name)
}

impl Registration {
    pub(super) fn of(arguments: Node<'_>, source: &Source<'_>, is_hook: bool) -> Self {
        let (key_name, priority_index) = if is_hook { ("event", 2) } else { ("name", 5) };
        let names =
            argument(arguments, source, 0, key_name).and_then(|node| names_of(node, source));
        let priority = argument(arguments, source, priority_index, "priority")
            .map_or(Some("normal"), |node| priority_name(source.text_of(node)));

        Self {
            is_hook,
            names,
            priority,
        }
    }

    fn may_share_order_with(&self, other: &Self) -> bool {
        let same_key = match (&self.names, &other.names) {
            (Some(own), Some(others)) => own.iter().any(|name| others.contains(name)),
            _ => true,
        };
        let same_priority =
            self.priority.is_none() || other.priority.is_none() || self.priority == other.priority;

        self.is_hook == other.is_hook && same_key && same_priority
    }
}

/// Decorated handlers that were registered in another order than the alphabetical one of their methods.
pub(super) fn reordered_decorators(conversions: &[Conversion<'_>]) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for (index, earlier) in conversions.iter().enumerate() {
        for later in &conversions[index + 1..] {
            if earlier.method > later.method
                && earlier
                    .registration
                    .may_share_order_with(&later.registration)
            {
                warnings.push(Warning::new(
                    line_of(later.statement),
                    format!(
                        "{} and {} share an event or command and its priority; decorated handlers are registered in \
                         the alphabetical order of their methods, so {} now runs before {}",
                        earlier.method, later.method, later.method, earlier.method
                    ),
                ));
            }
        }
    }

    warnings
}

/// Registrations that stay in `__init__` after a converted one. They are registered before all decorated handlers now.
pub(super) fn overtaking_registrations(
    init: Node<'_>,
    conversions: &[Conversion<'_>],
    source: &Source<'_>,
) -> Vec<Warning> {
    let mut warnings = Vec::new();
    walk(init, &mut |node| {
        let Some(call) = method_call(node) else {
            return;
        };
        let kind = source.text_of(call.name);
        let is_hook = kind == "add_hook";
        if source.text_of(call.receiver) != "self" || !(is_hook || kind == "add_command") {
            return;
        }
        let converted_here = conversions.iter().any(|conversion| {
            conversion.statement.start_byte() <= node.start_byte()
                && node.end_byte() <= conversion.statement.end_byte()
        });
        if converted_here {
            return;
        }

        let kept = Registration::of(call.arguments, source, is_hook);
        if let Some(overtaken) = conversions.iter().find(|conversion| {
            conversion.statement.start_byte() < node.start_byte()
                && conversion.registration.may_share_order_with(&kept)
        }) {
            warnings.push(Warning::new(
                line_of(node),
                format!(
                    "this registration now runs before the decorated handler {}, which has the same event or command \
                     and priority and was registered first before",
                    overtaken.method
                ),
            ));
        }
    });

    warnings
}
