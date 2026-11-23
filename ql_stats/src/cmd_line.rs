use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use ql_zmq::servers::{
    StatsEndpoint, password_is_exposed, save_stats_endpoint, server_file, stats_endpoint,
};

use crate::format::{ColorChoice, OutputFormat};

const DEFAULT_HOST: &str = "tcp://127.0.0.1:27961";

/// QuakeLive server stats
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub(crate) struct CommandLineOptions {
    /// Name of a server file `servers/<name>.toml` in the config directory (the one of `ql_rcon`): its `[stats]` table
    /// gives host and password where no option is given. With only `[stats] port` the host of the rcon connection is used.
    pub(crate) server: Option<String>,
    /// ZMQ URI to connect to, `tcp://127.0.0.1:27961` by default.
    #[arg(long)]
    pub(crate) host: Option<String>,
    /// The ZMQ password; none by default.
    #[arg(long)]
    pub(crate) password: Option<String>,
    /// Save host and password in the table `[stats]` of `servers/<name>.toml` (readable for the owner only) and connect
    #[arg(long, value_name = "NAME")]
    pub(crate) save_server: Option<String>,
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

fn with_scheme(host: String) -> String {
    if host.contains("://") {
        host
    } else {
        format!("tcp://{host}")
    }
}

fn open_server_file(name: &str) -> Result<PathBuf> {
    let path = server_file(name).ok_or_else(|| {
        anyhow!("'{name}' is not a valid server name or there is no config directory")
    })?;
    if !path.exists() {
        bail!(
            "no server file {}; create it with --save-server {name} and the connection options",
            path.display()
        );
    }

    Ok(path)
}

/// Reads the options and the server file they name.
pub(crate) fn parse() -> Result<CommandLineOptions> {
    let cli = CommandLineOptions::parse();
    let endpoint = match cli.server.as_deref() {
        Some(name) => {
            let path = open_server_file(name)?;
            if password_is_exposed(&path) {
                eprintln!(
                    "Warning: {} holds a password and can be read by other users, restrict it with chmod 600.",
                    path.display()
                );
            }
            stats_endpoint(&path).with_context(|| format!("could not read {}", path.display()))?
        }
        None => StatsEndpoint::default(),
    };
    let save_as = cli.save_server.clone();
    let options = cli.with_server_file(endpoint);

    if let Some(name) = save_as {
        let path = server_file(&name).ok_or_else(|| {
            anyhow!("'{name}' is not a valid server name or there is no config directory")
        })?;
        save_stats_endpoint(
            &path,
            &StatsEndpoint {
                host: Some(options.host().to_string()),
                password: Some(options.password().to_string()),
            },
        )
        .with_context(|| format!("could not save {}", path.display()))?;
        eprintln!("Saved the stats connection in {}.", path.display());
    }

    Ok(options)
}

impl CommandLineOptions {
    /// Options win over the server file, the server file wins over the defaults.
    fn with_server_file(mut self, endpoint: StatsEndpoint) -> Self {
        self.host = self.host.or(endpoint.host).map(with_scheme);
        self.password = self.password.or(endpoint.password);
        self
    }

    /// The ZMQ URI to connect to.
    pub(crate) fn host(&self) -> &str {
        self.host.as_deref().unwrap_or(DEFAULT_HOST)
    }

    /// The ZMQ password, empty for none.
    pub(crate) fn password(&self) -> &str {
        self.password.as_deref().unwrap_or_default()
    }

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
    use ql_zmq::servers::StatsEndpoint;

    use super::CommandLineOptions;
    use crate::format::{ColorChoice, OutputFormat};

    #[test]
    fn defaults_connect_to_the_local_server() {
        let args = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");

        assert_eq!(args.host(), "tcp://127.0.0.1:27961");
        assert_eq!(args.password(), "");
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

        assert_eq!(args.host(), "tcp://192.0.2.1:27960");
        assert_eq!(args.password(), "secret");
        assert_eq!(args.effective_format(), OutputFormat::Pretty);
        assert_eq!(args.types, ["PLAYER_DEATH", "ROUND_OVER"]);
        assert_eq!(args.exclude, ["PLAYER_STATS"]);
        assert_eq!(args.color, ColorChoice::Never);
        assert!(args.timestamp);
    }

    fn endpoint(host: Option<&str>, password: Option<&str>) -> StatsEndpoint {
        StatsEndpoint {
            host: host.map(str::to_string),
            password: password.map(str::to_string),
        }
    }

    #[test]
    fn the_server_file_fills_what_the_options_leave_out() {
        let args = CommandLineOptions::try_parse_from(["ql_stats", "arena"])
            .expect("the options are valid")
            .with_server_file(endpoint(Some("192.0.2.1:27962"), Some("stats")));

        assert_eq!(args.host(), "tcp://192.0.2.1:27962");
        assert_eq!(args.password(), "stats");
    }

    #[test]
    fn options_win_over_the_server_file() {
        let args = CommandLineOptions::try_parse_from([
            "ql_stats",
            "arena",
            "--host",
            "tcp://192.0.2.9:30000",
            "--password",
            "mine",
        ])
        .expect("the options are valid")
        .with_server_file(endpoint(Some("tcp://192.0.2.1:27962"), Some("stats")));

        assert_eq!(args.host(), "tcp://192.0.2.9:30000");
        assert_eq!(args.password(), "mine");
    }

    #[test]
    fn a_server_file_without_stats_keeps_the_defaults() {
        let args = CommandLineOptions::try_parse_from(["ql_stats", "arena"])
            .expect("the options are valid")
            .with_server_file(StatsEndpoint::default());

        assert_eq!(args.host(), "tcp://127.0.0.1:27961");
        assert_eq!(args.password(), "");
    }

    #[test]
    fn the_format_is_chosen_by_name() {
        let args = CommandLineOptions::try_parse_from(["ql_stats", "--format", "summary"])
            .expect("the options are valid");

        assert_eq!(args.effective_format(), OutputFormat::Summary);
    }
}
