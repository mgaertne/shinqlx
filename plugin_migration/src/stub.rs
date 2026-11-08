use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use tree_sitter::Node;

use crate::source::Source;

const STUB: &str = include_str!("../../python/shinqlx/_shinqlx.pyi");

struct Member {
    name: String,
    value: Option<String>,
}

/// The names that the type stub of shinqlx declares: module constants and class members.
pub(crate) struct Stub {
    constants: HashSet<String>,
    members: HashMap<String, Vec<Member>>,
}

fn declared_name<'a>(statement: Node<'_>, source: &Source<'a>) -> Option<&'a str> {
    let assignment = statement
        .named_child(0)
        .filter(|node| node.kind() == "assignment")?;
    let name = assignment.child_by_field_name("left")?;
    (name.kind() == "identifier").then(|| source.text_of(name))
}

fn string_value(statement: Node<'_>, source: &Source<'_>) -> Option<String> {
    let value = statement
        .named_child(0)?
        .child_by_field_name("right")
        .filter(|node| node.kind() == "string")?;
    Some(source.text_of(value).trim_matches(['"', '\'']).to_owned())
}

fn class_members(class: Node<'_>, source: &Source<'_>) -> Option<(String, Vec<Member>)> {
    let name = source
        .text_of(class.child_by_field_name("name")?)
        .to_owned();
    let body = class.child_by_field_name("body")?;
    let mut cursor = body.walk();
    let members = body
        .named_children(&mut cursor)
        .filter(|statement| statement.kind() == "expression_statement")
        .filter_map(|statement| {
            Some(Member {
                name: declared_name(statement, source)?.to_owned(),
                value: string_value(statement, source),
            })
        })
        .collect();
    Some((name, members))
}

impl Stub {
    fn parse(text: &str) -> Self {
        let source = Source::parse(text).expect("the type stub of shinqlx is valid python");
        let mut constants = HashSet::new();
        let mut members = HashMap::new();

        let root = source.root();
        let mut cursor = root.walk();
        for statement in root.named_children(&mut cursor) {
            let class = match statement.kind() {
                "expression_statement" => {
                    constants.extend(declared_name(statement, &source).map(str::to_owned));
                    None
                }
                "class_definition" => Some(statement),
                "decorated_definition" => statement.child_by_field_name("definition"),
                _ => None,
            };
            members.extend(class.and_then(|class| class_members(class, &source)));
        }
        Self { constants, members }
    }

    pub(crate) fn has_constant(&self, name: &str) -> bool {
        self.constants.contains(name)
    }

    pub(crate) fn has_member(&self, class: &str, member: &str) -> bool {
        self.members
            .get(class)
            .is_some_and(|members| members.iter().any(|candidate| candidate.name == member))
    }

    pub(crate) fn has_class(&self, class: &str) -> bool {
        self.members.contains_key(class)
    }

    /// The member of `class` whose string value is `value`.
    pub(crate) fn member_with_value(&self, class: &str, value: &str) -> Option<&str> {
        self.members
            .get(class)?
            .iter()
            .find(|member| member.value.as_deref() == Some(value))
            .map(|member| member.name.as_str())
    }

    #[cfg(test)]
    pub(crate) fn constants(&self) -> impl Iterator<Item = &str> {
        self.constants.iter().map(String::as_str)
    }
}

/// The stub the tool was built with.
pub(crate) fn shinqlx_stub() -> &'static Stub {
    static PARSED: LazyLock<Stub> = LazyLock::new(|| Stub::parse(STUB));
    &PARSED
}

#[cfg(test)]
mod tests {
    use super::shinqlx_stub;

    #[test]
    fn knows_constants_and_class_members() {
        let stub = shinqlx_stub();
        assert!(stub.has_constant("RET_STOP_ALL"));
        assert!(stub.has_member("ReturnCodes", "STOP_ALL"));
        assert!(stub.has_member("Events", "CHAT"));
        assert!(!stub.has_constant("ReturnCodes"));
        assert!(!stub.has_member("ReturnCodes", "UNKNOWN"));
    }

    #[test]
    fn maps_string_values_to_members() {
        let stub = shinqlx_stub();
        assert_eq!(
            stub.member_with_value("Events", "player_connect"),
            Some("PLAYER_CONNECT")
        );
        assert_eq!(stub.member_with_value("Events", "no_such_event"), None);
    }
}
