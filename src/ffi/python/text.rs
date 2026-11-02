use core::{ops::Deref, str::FromStr};

use itertools::Itertools;
use log::*;
use pyo3::types::{IntoPyDict, PyDict};

use super::prelude::*;

fn is_color_tag_digit(byte: Option<&u8>) -> bool {
    byte.is_some_and(|digit| (b'0'..=b'7').contains(digit))
}

/// Removes all color tags (a caret followed by a digit from 0 to 7) from a text.
pub(crate) fn clean_text<T>(text: &T) -> String
where
    T: AsRef<str>,
{
    let mut rest = text.as_ref();
    let mut cleaned = String::with_capacity(rest.len());
    while let Some(caret) = rest.find('^') {
        cleaned.push_str(&rest[..caret]);
        let after_caret = &rest[caret + 1..];
        if is_color_tag_digit(after_caret.as_bytes().first()) {
            rest = &after_caret[1..];
        } else {
            cleaned.push('^');
            rest = after_caret;
        }
    }
    cleaned.push_str(rest);
    cleaned
}

/// Returns the last color tag (a caret followed by a digit from 0 to 7) of a text.
pub(crate) fn last_color_tag(text: &str) -> Option<&str> {
    text.rmatch_indices('^').find_map(|(caret, _)| {
        is_color_tag_digit(text.as_bytes().get(caret + 1)).then(|| &text[caret..caret + 2])
    })
}

#[cfg(test)]
mod color_tag_tests {
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use rstest::rstest;

    use super::{clean_text, last_color_tag};

    #[rstest]
    #[case("", "")]
    #[case("PlayerName", "PlayerName")]
    #[case("^1Red^7Player^4Name", "RedPlayerName")]
    #[case("^^1a", "^a")]
    #[case("a^", "a^")]
    #[case("^", "^")]
    #[case("^8x^9y", "^8x^9y")]
    #[case("^7^7", "")]
    #[case("^a^1", "^a")]
    #[case("ä^1ö^2ü", "äöü")]
    #[case("^^^0^", "^^^")]
    fn clean_text_removes_color_tags(#[case] text: &str, #[case] expected: &str) {
        let color_tag_regex = Regex::new(r"\^[0-7]").unwrap();

        assert_eq!(clean_text(&text), expected);
        assert_eq!(clean_text(&text), color_tag_regex.replace_all(text, ""));
    }

    #[rstest]
    #[case("", None)]
    #[case("no tags", None)]
    #[case("^1red", Some("^1"))]
    #[case("^1red ^2green", Some("^2"))]
    #[case("^1red ^", Some("^1"))]
    #[case("^1red ^9", Some("^1"))]
    #[case("^^3x", Some("^3"))]
    #[case("ä^4ö", Some("^4"))]
    #[case("^", None)]
    #[case("^8", None)]
    fn last_color_tag_finds_the_last_tag(#[case] text: &str, #[case] expected: Option<&str>) {
        let color_tag_regex = Regex::new(r"\^[0-7]").unwrap();

        assert_eq!(last_color_tag(text), expected);
        assert_eq!(
            last_color_tag(text),
            color_tag_regex
                .find_iter(text)
                .last()
                .map(|color_tag| color_tag.as_str())
        );
    }
}

pub(crate) fn parse_variables(varstr: &str) -> ParsedVariables {
    varstr
        .parse::<ParsedVariables>()
        .unwrap_or(ParsedVariables { items: vec![] })
}

pub(crate) struct ParsedVariables {
    items: Vec<(String, String)>,
}

impl FromStr for ParsedVariables {
    type Err = &'static str;

    fn from_str(varstr: &str) -> Result<Self, Self::Err> {
        if varstr.trim().is_empty() {
            return Ok(Self { items: vec![] });
        }

        let stripped_varstr = varstr.strip_prefix('\\').unwrap_or(varstr).to_string();

        let varstr_vec = stripped_varstr
            .split('\\')
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        if varstr_vec.len() % 2 == 1 {
            warn!(target: "shinqlx", "Uneven number of keys and values: {varstr}");
        }
        Ok(Self {
            items: varstr_vec.iter().cloned().tuples().collect(),
        })
    }
}

impl From<ParsedVariables> for String {
    fn from(value: ParsedVariables) -> Self {
        value
            .items
            .iter()
            .map(|(key, value)| format!(r"\{key}\{value}"))
            .collect::<String>()
    }
}

impl<'py> IntoPyDict<'py> for ParsedVariables {
    fn into_py_dict(self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.items.into_py_dict(py)
    }
}

impl Deref for ParsedVariables {
    type Target = Vec<(String, String)>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl ParsedVariables {
    pub(crate) fn contains<T>(&self, item: T) -> bool
    where
        T: AsRef<str>,
    {
        self.items.iter().any(|(key, _value)| *key == item.as_ref())
    }

    pub fn get<T>(&self, item: T) -> Option<String>
    where
        T: AsRef<str>,
    {
        self.items
            .iter()
            .find(|(key, _value)| *key == item.as_ref())
            .map(|(_key, value)| value)
            .cloned()
    }

    pub fn set(&mut self, item: &str, value: &str) {
        let mut new_items = self
            .items
            .iter()
            .filter(|(key, _value)| key != item)
            .cloned()
            .collect::<Vec<_>>();
        new_items.push((item.into(), value.into()));
        self.items = new_items;
    }
}

#[cfg(test)]
mod parsed_variables_tests {
    use core::str::FromStr;

    use super::ParsedVariables;

    #[test]
    fn parse_variables_with_space() {
        let variables = ParsedVariables::from_str(r"\name\Unnamed Player\country\de")
            .expect("this should not happen");
        assert!(
            variables
                .get("name")
                .is_some_and(|value| value == "Unnamed Player")
        );
        assert!(variables.get("country").is_some_and(|value| value == "de"));
    }
}

/// Parses strings of key-value pairs delimited by r"\" and puts
/// them into a dictionary.
#[pyfunction]
#[pyo3(name = "parse_variables")]
#[pyo3(signature = (varstr, ordered = false), text_signature = "(varstr, ordered = False)")]
pub(super) fn pyshinqlx_parse_variables<'py>(
    py: Python<'py>,
    varstr: &str,
    #[allow(unused_variables, reason = "kept for the signature of minqlx")] ordered: bool,
) -> Bound<'py, PyDict> {
    let parsed_variables = py.detach(|| parse_variables(varstr));
    parsed_variables
        .into_py_dict(py)
        .unwrap_or_else(|_| PyDict::new(py))
}

#[cfg(test)]
mod pyshinqlx_parse_variables_test {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::pyshinqlx_parse_variables;
    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn parse_variables_with_space(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let variables = pyshinqlx_parse_variables(py, r"\name\Unnamed Player\country\de", true);
            assert!(variables.get_item(intern!(py, "name")).is_ok_and(|value| {
                value.is_some_and(|str_value| str_value.to_string() == "Unnamed Player")
            }));
            assert!(variables
                .get_item(intern!(py, "country"))
                .is_ok_and(|value| value.is_some_and(|str_value| str_value.to_string() == "de")));
        });
    }
}
