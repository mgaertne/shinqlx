//! Turns the stats messages of the server into the lines `ql_stats` prints.
use std::io::{IsTerminal, stdout};

use clap::ValueEnum;
use ql_zmq::quake_colors::{ql_rgb, split_ql_colors};
use serde_json::Value;

use crate::cmd_line::CommandLineOptions;

/// How a stats message is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
    /// One line of compact json.
    Json,
    /// Indented json.
    Pretty,
    /// One readable line for the common event types, compact json for the others.
    Summary,
}

/// When the output is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ColorChoice {
    /// Only when the output goes to a terminal and `NO_COLOR` is not set.
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub(crate) fn enabled(self) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    enabled: bool,
}

const KEY: &str = "36";
const TEXT: &str = "32";
const NUMBER: &str = "33";
const LITERAL: &str = "35";

impl Palette {
    fn paint(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

/// Filters the messages by their type and renders the remaining ones.
#[derive(Debug, Clone)]
pub(crate) struct Renderer {
    format: OutputFormat,
    include: Vec<String>,
    exclude: Vec<String>,
    palette: Palette,
}

impl Renderer {
    pub(crate) fn new(args: &CommandLineOptions, color: bool) -> Self {
        Self {
            format: args.effective_format(),
            include: upper_case(&args.types),
            exclude: upper_case(&args.exclude),
            palette: Palette { enabled: color },
        }
    }

    /// The line for a message, or `None` when the type filter drops it. Text that is no json passes through.
    pub(crate) fn render(&self, msg: &str) -> Option<String> {
        let Ok(parsed) = serde_json::from_str::<Value>(msg) else {
            return Some(msg.to_string());
        };
        let kind = parsed.get("TYPE").and_then(Value::as_str);
        if kind.is_some_and(|kind| !self.shows(kind)) {
            return None;
        }

        let summary = match (self.format, kind) {
            (OutputFormat::Summary, Some(kind)) => parsed
                .get("DATA")
                .and_then(|data| self.summarize(kind, data)),
            _ => None,
        };
        Some(summary.unwrap_or_else(|| {
            let mut line = String::new();
            write_value(
                &mut line,
                &parsed,
                self.format == OutputFormat::Pretty,
                0,
                self.palette,
            );
            line
        }))
    }

    fn shows(&self, kind: &str) -> bool {
        let kind = kind.to_uppercase();
        (self.include.is_empty() || self.include.contains(&kind)) && !self.exclude.contains(&kind)
    }

    fn summarize(&self, kind: &str, data: &Value) -> Option<String> {
        let body = match kind {
            "PLAYER_DEATH" | "PLAYER_KILL" => death_text(data, self.palette)?,
            "PLAYER_MEDAL" => format!(
                "{} got {}",
                name(data, "/NAME", self.palette)?,
                text(data, "/MEDAL")?
            ),
            "PLAYER_SWITCHTEAM" => format!(
                "{}: {} -> {}",
                name(data, "/KILLER/NAME", self.palette)?,
                text(data, "/KILLER/OLD_TEAM")?,
                text(data, "/KILLER/TEAM")?
            ),
            "PLAYER_STATS" => format!(
                "{} kills {} deaths {} score {}",
                name(data, "/NAME", self.palette)?,
                number(data, "/KILLS")?,
                number(data, "/DEATHS")?,
                number(data, "/SCORE")?
            ),
            "ROUND_OVER" => format!(
                "round {}, {} won",
                number(data, "/ROUND")?,
                text(data, "/TEAM_WON")?
            ),
            "MATCH_STARTED" => format!(
                "{} on {}",
                text(data, "/FACTORY_TITLE")?,
                text(data, "/MAP")?
            ),
            "MATCH_REPORT" => format!(
                "{} on {}: {}, score {}:{}, {} s",
                text(data, "/FACTORY")?,
                text(data, "/MAP")?,
                text(data, "/EXIT_MSG")?,
                number(data, "/TSCORE0")?,
                number(data, "/TSCORE1")?,
                number(data, "/GAME_LENGTH")?
            ),
            _ => return None,
        };
        let warmup = if data.get("WARMUP").and_then(Value::as_bool) == Some(true) {
            " [warmup]"
        } else {
            ""
        };
        let label = self.palette.paint(kind_color(kind), &format!("{kind:<17}"));
        Some(format!("{label}  {body}{warmup}"))
    }
}

fn upper_case(types: &[String]) -> Vec<String> {
    types.iter().map(|kind| kind.to_uppercase()).collect()
}

fn kind_color(kind: &str) -> &'static str {
    match kind {
        "PLAYER_DEATH" | "PLAYER_KILL" => "31",
        "ROUND_OVER" | "MATCH_STARTED" | "MATCH_REPORT" => "33",
        "PLAYER_MEDAL" => "32",
        _ => "34",
    }
}

fn text<'a>(data: &'a Value, path: &str) -> Option<&'a str> {
    data.pointer(path).and_then(Value::as_str)
}

fn number(data: &Value, path: &str) -> Option<String> {
    data.pointer(path)
        .and_then(Value::as_number)
        .map(ToString::to_string)
}

/// A player name; in the colors of the game when the output is colored, otherwise without the color codes.
fn name(data: &Value, path: &str, palette: Palette) -> Option<String> {
    let colored = split_ql_colors(text(data, path)?)
        .into_iter()
        .map(|(code, part)| match ql_rgb(code) {
            _ if !palette.enabled || code == "reset" => part.to_string(),
            Some((red, green, blue)) => palette.paint(&format!("38;2;{red};{green};{blue}"), part),
            None => palette.paint("30", part),
        })
        .collect();
    Some(colored)
}

fn death_text(data: &Value, palette: Palette) -> Option<String> {
    let victim = name(data, "/VICTIM/NAME", palette)?;
    let cause = text(data, "/MOD")?;
    if data.get("SUICIDE").and_then(Value::as_bool) == Some(true) {
        return Some(format!("{victim} suicide ({cause})"));
    }
    let teamkill = if data.get("TEAMKILL").and_then(Value::as_bool) == Some(true) {
        ", teamkill"
    } else {
        ""
    };
    Some(format!(
        "{victim} <- {} ({cause}{teamkill})",
        name(data, "/KILLER/NAME", palette)?
    ))
}

fn quote(text: &str) -> String {
    Value::String(text.to_string()).to_string()
}

fn write_value(out: &mut String, value: &Value, pretty: bool, level: usize, palette: Palette) {
    match value {
        Value::Null => out.push_str(&palette.paint(LITERAL, "null")),
        Value::Bool(flag) => out.push_str(&palette.paint(LITERAL, &flag.to_string())),
        Value::Number(number) => out.push_str(&palette.paint(NUMBER, &number.to_string())),
        Value::String(string) => out.push_str(&palette.paint(TEXT, &quote(string))),
        Value::Array(items) => write_container(
            out,
            ('[', ']'),
            items.iter().map(|item| (None, item)).collect(),
            (pretty, level),
            palette,
        ),
        Value::Object(map) => write_container(
            out,
            ('{', '}'),
            map.iter()
                .map(|(key, item)| (Some(key.as_str()), item))
                .collect(),
            (pretty, level),
            palette,
        ),
    }
}

fn write_container(
    out: &mut String,
    (open, close): (char, char),
    entries: Vec<(Option<&str>, &Value)>,
    (pretty, level): (bool, usize),
    palette: Palette,
) {
    out.push(open);
    if entries.is_empty() {
        out.push(close);
        return;
    }
    for (index, (key, value)) in entries.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        if pretty {
            out.push('\n');
            out.push_str(&"  ".repeat(level + 1));
        }
        if let Some(key) = key {
            out.push_str(&palette.paint(KEY, &quote(key)));
            out.push(':');
            if pretty {
                out.push(' ');
            }
        }
        write_value(out, value, pretty, level + 1, palette);
    }
    if pretty {
        out.push('\n');
        out.push_str(&"  ".repeat(level));
    }
    out.push(close);
}

#[cfg(test)]
mod format_tests {
    use clap::Parser;

    use super::{ColorChoice, Renderer};
    use crate::cmd_line::CommandLineOptions;

    fn renderer(extra: &[&str], color: bool) -> Renderer {
        let args = CommandLineOptions::try_parse_from(
            ["ql_stats"].into_iter().chain(extra.iter().copied()),
        )
        .expect("the options are valid");
        Renderer::new(&args, color)
    }

    const DEATH: &str = r#"{"DATA":{"KILLER":{"NAME":"^1Alice^7"},"MOD":"ROCKET","SUICIDE":false,"TEAMKILL":false,"VICTIM":{"NAME":"Bob"},"WARMUP":true},"TYPE":"PLAYER_DEATH"}"#;

    #[test]
    fn json_keeps_the_order_of_the_server() {
        let line =
            renderer(&[], false).render(r#"{ "TYPE": "A", "DATA": { "x": [1, 2.5, null] } }"#);

        assert_eq!(
            line.as_deref(),
            Some(r#"{"TYPE":"A","DATA":{"x":[1,2.5,null]}}"#)
        );
    }

    #[test]
    fn pretty_indents_like_serde() {
        let line =
            renderer(&["--format", "pretty"], false).render(r#"{"a":1,"b":[],"c":{"d":true}}"#);

        assert_eq!(
            line.as_deref(),
            Some("{\n  \"a\": 1,\n  \"b\": [],\n  \"c\": {\n    \"d\": true\n  }\n}")
        );
    }

    #[test]
    fn the_old_pretty_print_switch_still_works() {
        let line = renderer(&["--pretty-print"], false).render(r#"{"a":1}"#);

        assert_eq!(line.as_deref(), Some("{\n  \"a\": 1\n}"));
    }

    #[test]
    fn other_text_passes_through() {
        let line = renderer(&["--type", "ROUND_OVER"], false).render("no json");

        assert_eq!(line.as_deref(), Some("no json"));
    }

    #[test]
    fn the_type_filter_keeps_and_drops_messages() {
        let only_deaths = renderer(&["--type", "player_death,player_kill"], false);
        let no_deaths = renderer(&["--exclude", "PLAYER_DEATH"], false);

        assert!(only_deaths.render(DEATH).is_some());
        assert!(
            only_deaths
                .render(r#"{"TYPE":"ROUND_OVER","DATA":{}}"#)
                .is_none()
        );
        assert!(no_deaths.render(DEATH).is_none());
        assert!(
            no_deaths
                .render(r#"{"TYPE":"ROUND_OVER","DATA":{}}"#)
                .is_some()
        );
    }

    #[test]
    fn colored_summaries_show_the_names_in_the_colors_of_the_game() {
        let line = renderer(&["--format", "summary"], true).render(
            r#"{"TYPE":"PLAYER_MEDAL","DATA":{"NAME":"^1Al^4ice^7","MEDAL":"IMPRESSIVE"}}"#,
        );

        assert_eq!(
            line.as_deref(),
            Some(
                "\x1b[32mPLAYER_MEDAL     \x1b[0m  \x1b[38;2;255;0;0mAl\x1b[0m\x1b[38;2;0;0;255mice\x1b[0m got IMPRESSIVE"
            )
        );
    }

    #[test]
    fn the_summary_names_killer_and_victim_without_color_codes() {
        let line = renderer(&["--format", "summary"], false).render(DEATH);

        assert_eq!(
            line.as_deref(),
            Some("PLAYER_DEATH       Bob <- Alice (ROCKET) [warmup]")
        );
    }

    #[test]
    fn the_summary_of_a_round_and_a_suicide() {
        let summary = renderer(&["--format", "summary"], false);

        assert_eq!(
            summary
                .render(
                    r#"{"TYPE":"ROUND_OVER","DATA":{"ROUND":2,"TEAM_WON":"RED","WARMUP":false}}"#
                )
                .as_deref(),
            Some("ROUND_OVER         round 2, RED won")
        );
        assert_eq!(
            summary
                .render(r#"{"TYPE":"PLAYER_DEATH","DATA":{"VICTIM":{"NAME":"Bob"},"MOD":"LAVA","SUICIDE":true}}"#)
                .as_deref(),
            Some("PLAYER_DEATH       Bob suicide (LAVA)")
        );
    }

    #[test]
    fn unknown_types_and_missing_fields_fall_back_to_json() {
        let summary = renderer(&["--format", "summary"], false);

        assert_eq!(
            summary
                .render(r#"{"TYPE":"NEW_TYPE","DATA":{"a":1}}"#)
                .as_deref(),
            Some(r#"{"TYPE":"NEW_TYPE","DATA":{"a":1}}"#)
        );
        assert_eq!(
            summary
                .render(r#"{"TYPE":"ROUND_OVER","DATA":{}}"#)
                .as_deref(),
            Some(r#"{"TYPE":"ROUND_OVER","DATA":{}}"#)
        );
    }

    #[test]
    fn color_wraps_keys_and_values() {
        let line = renderer(&[], true).render(r#"{"a":"x","b":1,"c":null}"#);

        assert_eq!(
            line.as_deref(),
            Some(
                "{\x1b[36m\"a\"\x1b[0m:\x1b[32m\"x\"\x1b[0m,\x1b[36m\"b\"\x1b[0m:\x1b[33m1\x1b[0m,\x1b[36m\"c\"\x1b[0m:\x1b[35mnull\x1b[0m}"
            )
        );
    }

    #[test]
    fn color_choice_can_be_forced() {
        assert!(ColorChoice::Always.enabled());
        assert!(!ColorChoice::Never.enabled());
    }
}
