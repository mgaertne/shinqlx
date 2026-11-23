use clap::Parser;

use crate::format::{ColorChoice, OutputFormat};

/// QuakeLive server stats
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub(crate) struct CommandLineOptions {
    /// ZMQ URI to connect to.
    #[arg(long, default_value = "tcp://127.0.0.1:27961")]
    pub(crate) host: String,
    /// The ZMQ password.
    #[arg(long, default_value = "")]
    pub(crate) password: String,
    /// Pretty print received json data (same as `--format pretty`)
    #[arg(long, hide = true)]
    pub(crate) pretty_print: bool,
    /// How the messages are printed.
    #[arg(long, value_enum, default_value = "json")]
    pub(crate) format: OutputFormat,
    /// Only show these message types, for example `PLAYER_DEATH,ROUND_OVER`.
    #[arg(long = "type", value_delimiter = ',')]
    pub(crate) types: Vec<String>,
    /// Hide these message types.
    #[arg(long, value_delimiter = ',')]
    pub(crate) exclude: Vec<String>,
    /// When the output is colored.
    #[arg(long, value_enum, default_value = "auto")]
    pub(crate) color: ColorChoice,
    /// Put the time of receipt before every message.
    #[arg(long)]
    pub(crate) timestamp: bool,
}

impl CommandLineOptions {
    /// The output format, where the older `--pretty-print` switch means `pretty`.
    pub(crate) fn effective_format(&self) -> OutputFormat {
        if self.pretty_print {
            OutputFormat::Pretty
        } else {
            self.format
        }
    }
}

#[cfg(test)]
mod cmd_line_tests {
    use clap::Parser;

    use super::CommandLineOptions;
    use crate::format::{ColorChoice, OutputFormat};

    #[test]
    fn defaults_connect_to_the_local_server() {
        let args = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");

        assert_eq!(args.host, "tcp://127.0.0.1:27961");
        assert_eq!(args.password, "");
        assert_eq!(args.effective_format(), OutputFormat::Json);
        assert!(args.types.is_empty());
        assert!(args.exclude.is_empty());
        assert_eq!(args.color, ColorChoice::Auto);
        assert!(!args.timestamp);
    }

    #[test]
    fn options_override_the_defaults() {
        let args = CommandLineOptions::try_parse_from([
            "ql_stats",
            "--host",
            "tcp://192.0.2.1:27960",
            "--password",
            "secret",
            "--pretty-print",
            "--type",
            "PLAYER_DEATH,ROUND_OVER",
            "--exclude",
            "PLAYER_STATS",
            "--color",
            "never",
            "--timestamp",
        ])
        .expect("the options are valid");

        assert_eq!(args.host, "tcp://192.0.2.1:27960");
        assert_eq!(args.password, "secret");
        assert_eq!(args.effective_format(), OutputFormat::Pretty);
        assert_eq!(args.types, ["PLAYER_DEATH", "ROUND_OVER"]);
        assert_eq!(args.exclude, ["PLAYER_STATS"]);
        assert_eq!(args.color, ColorChoice::Never);
        assert!(args.timestamp);
    }

    #[test]
    fn the_format_is_chosen_by_name() {
        let args = CommandLineOptions::try_parse_from(["ql_stats", "--format", "summary"])
            .expect("the options are valid");

        assert_eq!(args.effective_format(), OutputFormat::Summary);
    }
}
