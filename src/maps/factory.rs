//! Parser for the factory files (`scripts/factories.txt`, `scripts/*.factories`): a JSON array of gametype presets.

use alloc::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use super::strip_control;

/// A factory as the file describes it; `basegt` is kept as written, `cvars` are sorted by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Factory {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) basegt: Option<String>,
    pub(crate) cvars: BTreeMap<String, String>,
}

/// The usable factories of a file and how many entries had to be skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ParsedFactories {
    pub(crate) factories: Vec<Factory>,
    pub(crate) skipped: usize,
}

/// The file is not a JSON array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactoryFileError(pub(crate) String);

#[derive(Deserialize)]
struct RawFactory {
    id: String,
    title: Option<String>,
    author: Option<String>,
    description: Option<String>,
    basegt: Option<String>,
    #[serde(default)]
    cvars: BTreeMap<String, Value>,
}

impl RawFactory {
    fn into_factory(self) -> Option<Factory> {
        let id = strip_control(&self.id);
        if id.is_empty() {
            return None;
        }
        Some(Factory {
            id,
            title: self.title.as_deref().map(strip_control),
            author: self.author.as_deref().map(strip_control),
            description: self.description.as_deref().map(strip_control),
            basegt: self.basegt.as_deref().map(strip_control),
            cvars: self
                .cvars
                .into_iter()
                .filter_map(|(name, value)| Some((strip_control(&name), cvar_text(value)?)))
                .collect(),
        })
    }
}

fn cvar_text(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(strip_control(&text)),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(u8::from(flag).to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Parses a factory file. Entries without a usable `id` or of the wrong shape are skipped and counted, they do not
/// discard the rest. Cvar values given as numbers or booleans are written as text.
pub(crate) fn parse_factories(text: &str) -> Result<ParsedFactories, FactoryFileError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let entries = serde_json::from_str::<Vec<Value>>(text)
        .map_err(|error| FactoryFileError(error.to_string()))?;

    let mut parsed = ParsedFactories::default();
    for entry in entries {
        match serde_json::from_value::<RawFactory>(entry)
            .ok()
            .and_then(RawFactory::into_factory)
        {
            Some(factory) => parsed.factories.push(factory),
            None => parsed.skipped += 1,
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const SAMPLE: &str = r#"[
      {
        "cvars": { "timelimit": "15", "g_overtime": "0",	"g_voteFlags": "2048" },
        "author": "id Software",
        "description": "Classic CTF.",
        "basegt": "ctf",
        "id": "actf",
        "title": "Arena CTF"
      }
    ]"#;

    #[test]
    fn parses_a_real_style_entry_with_tabs() {
        let parsed = parse_factories(SAMPLE).unwrap();

        assert_eq!(parsed.skipped, 0);
        assert_eq!(
            parsed.factories,
            vec![Factory {
                id: "actf".into(),
                title: Some("Arena CTF".into()),
                author: Some("id Software".into()),
                description: Some("Classic CTF.".into()),
                basegt: Some("ctf".into()),
                cvars: [
                    ("g_overtime", "0"),
                    ("g_voteFlags", "2048"),
                    ("timelimit", "15")
                ]
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            }]
        );
    }

    #[test]
    fn optional_fields_may_be_missing_and_unknown_keys_are_ignored() {
        let parsed = parse_factories(r#"[{"id": "ca2", "future": [1, 2]}]"#).unwrap();

        let factory = &parsed.factories[0];
        assert_eq!(factory.id, "ca2");
        assert_eq!(factory.title, None);
        assert_eq!(factory.basegt, None);
        assert!(factory.cvars.is_empty());
    }

    #[test]
    fn cvar_numbers_and_booleans_become_text_and_other_values_are_dropped() {
        let parsed = parse_factories(
            r#"[{"id": "x", "cvars": {"a": 5, "b": 1.5, "c": true, "d": null, "e": [1], "f": "t\u0007"}}]"#,
        )
        .unwrap();

        let cvars = &parsed.factories[0].cvars;
        assert_eq!(cvars.get("a").map(String::as_str), Some("5"));
        assert_eq!(cvars.get("b").map(String::as_str), Some("1.5"));
        assert_eq!(cvars.get("c").map(String::as_str), Some("1"));
        assert_eq!(cvars.get("f").map(String::as_str), Some("t"));
        assert_eq!(cvars.len(), 4);
    }

    #[test]
    fn bad_entries_are_skipped_and_counted() {
        let parsed = parse_factories(
            r#"[{"id": "ok"}, {"title": "no id"}, {"id": ""}, 7, {"id": 3}, {"id": "ok2"}]"#,
        )
        .unwrap();

        let ids = parsed
            .factories
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["ok", "ok2"]);
        assert_eq!(parsed.skipped, 4);
    }

    #[test]
    fn strips_control_characters_and_a_bom() {
        let parsed =
            parse_factories("\u{feff}[{\"id\": \"a\\u0001b\", \"title\": \"^1Red\\n\"}]").unwrap();

        assert_eq!(parsed.factories[0].id, "ab");
        assert_eq!(parsed.factories[0].title.as_deref(), Some("^1Red"));
    }

    #[test]
    fn empty_array_is_fine_and_other_json_is_an_error() {
        assert_eq!(parse_factories("[]").unwrap(), ParsedFactories::default());
        assert!(parse_factories(r#"{"id": "x"}"#).is_err());
        assert!(parse_factories("[{").is_err());
        assert!(parse_factories("").is_err());
    }
}
