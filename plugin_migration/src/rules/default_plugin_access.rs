use super::{Rule, RuleOutput, Warning};
use crate::{
    source::{Source, line_of},
    tree::walk,
};

/// A default plugin that shinqlx provides in Rust, with the public members besides the
/// `cmd_*` and `handle_*` ones that other plugins can call.
struct NativePlugin {
    name: &'static str,
    members: &'static [&'static str],
}

const NATIVE_PLUGINS: [NativePlugin; 10] = [
    NativePlugin {
        name: "essentials",
        members: &["force"],
    },
    NativePlugin {
        name: "ban",
        members: &[
            "is_banned",
            "leave_status",
            "warn_player",
            "deliver_warning",
            "remember_players",
        ],
    },
    NativePlugin {
        name: "silence",
        members: &["is_silenced"],
    },
    NativePlugin {
        name: "names",
        members: &[],
    },
    NativePlugin {
        name: "clan",
        members: &[],
    },
    NativePlugin {
        name: "permission",
        members: &[],
    },
    NativePlugin {
        name: "plugin_manager",
        members: &[
            "reload_in_next_frame",
            "load_all_in_next_frame",
            "reload_all_in_next_frame",
        ],
    },
    NativePlugin {
        name: "workshop",
        members: &[],
    },
    NativePlugin {
        name: "motd",
        members: &["deliver_motd", "send_motd"],
    },
    NativePlugin {
        name: "log",
        members: &[],
    },
];

/// Warns about uses of the plugin interface that shinqlx does not provide in the same way.
pub(crate) struct DefaultPluginAccess;

fn is_provided(plugin: &NativePlugin, member: &str) -> bool {
    member.starts_with("cmd_") || member.starts_with("handle_") || plugin.members.contains(&member)
}

fn plugin_name_of_lookup<'a>(
    lookup: tree_sitter::Node<'_>,
    source: &Source<'a>,
) -> Option<&'a str> {
    if lookup.kind() != "subscript" {
        return None;
    }
    let plugins = lookup.child_by_field_name("value")?;
    let is_plugins = match plugins.kind() {
        "identifier" => source.text_of(plugins) == "plugins",
        "attribute" => plugins
            .child_by_field_name("attribute")
            .is_some_and(|name| source.text_of(name) == "plugins"),
        _ => false,
    };
    let key = lookup.child_by_field_name("subscript")?;
    (is_plugins && key.kind() == "string").then(|| source.text_of(key).trim_matches(['"', '\'']))
}

impl Rule for DefaultPluginAccess {
    fn name(&self) -> &'static str {
        "default-plugin-access"
    }

    fn apply(&self, source: &Source<'_>) -> RuleOutput {
        let mut output = RuleOutput::default();
        walk(source.root(), &mut |node| {
            if node.kind() != "attribute" {
                return;
            }
            let Some(member) = node.child_by_field_name("attribute") else {
                return;
            };
            let member_name = source.text_of(member);

            if member_name == "next_frame_tasks" {
                output.warnings.push(Warning::new(
                    line_of(node),
                    "next_frame_tasks is a queue.SimpleQueue in shinqlx (put/get_nowait), use the \
                     next_frame decorator instead",
                ));
                return;
            }

            let native_plugin = node
                .child_by_field_name("object")
                .and_then(|object| plugin_name_of_lookup(object, source))
                .and_then(|name| NATIVE_PLUGINS.iter().find(|plugin| plugin.name == name));
            if let Some(plugin) = native_plugin
                && !is_provided(plugin, member_name)
            {
                output.warnings.push(Warning::new(
                    line_of(node),
                    format!(
                        "the default plugin {} is written in Rust and does not provide `{member_name}`",
                        plugin.name
                    ),
                ));
            }
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{DefaultPluginAccess, NATIVE_PLUGINS};
    use crate::{rules::Rule, source::Source};

    fn warnings(text: &str) -> usize {
        let source = Source::parse(text).expect("the test source is valid python");
        let output = DefaultPluginAccess.apply(&source);
        assert!(output.edits.is_empty());
        output.warnings.len()
    }

    #[rstest]
    #[case("self.plugins['essentials'].mappool\n", 1)]
    #[case("self.plugins[\"essentials\"].parse_mappool(path)\n", 1)]
    #[case("minqlx.Plugin.plugins['essentials'].update_player(p)\n", 1)]
    #[case("plugins['ban'].is_banned(p)\n", 0)]
    #[case("self.plugins['silence'].is_silenced(p)\n", 0)]
    #[case("self.plugins['essentials'].force(1, 2)\n", 0)]
    #[case("self.plugins['essentials'].cmd_kick(p, m, c)\n", 0)]
    #[case("self.plugins['balance'].suggested_pair\n", 0)]
    #[case("self.plugins['fun'].play_sound(x)\n", 0)]
    #[case("self.other['essentials'].mappool\n", 0)]
    #[case("minqlx.next_frame_tasks.append(x)\n", 1)]
    fn counts_warnings(#[case] input: &str, #[case] expected: usize) {
        assert_eq!(warnings(input), expected);
    }

    #[test]
    fn every_listed_member_exists_in_the_rust_plugin() {
        for plugin in &NATIVE_PLUGINS {
            let file = if plugin.name == "log" {
                "chat_log"
            } else {
                plugin.name
            };
            let path = format!(
                "{}/../src/ffi/python/native_plugins/{file}.rs",
                env!("CARGO_MANIFEST_DIR"),
            );
            let rust_source = fs::read_to_string(&path).expect("could not read the rust plugin");
            for member in plugin.members {
                assert!(
                    rust_source.contains(&format!("fn {member}("))
                        || rust_source.contains(&format!("fn {member}<")),
                    "{} does not define {member}",
                    plugin.name
                );
            }
        }
    }
}
