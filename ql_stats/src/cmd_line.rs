use std::path::{Path, PathBuf};

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

type ServerFileLocator<'a> = &'a dyn Fn(&str) -> Option<PathBuf>;

fn path_of_server_file(name: &str, locate: ServerFileLocator) -> Result<PathBuf> {
    locate(name).ok_or_else(|| {
        anyhow!("'{name}' is not a valid server name or there is no config directory")
    })
}

fn open_server_file(name: &str, locate: ServerFileLocator) -> Result<PathBuf> {
    open_existing(name, path_of_server_file(name, locate)?)
}

fn open_existing(name: &str, path: PathBuf) -> Result<PathBuf> {
    if !path.exists() {
        bail!(
            "no server file {}; create it with --save-server {name} and the connection options",
            path.display()
        );
    }

    Ok(path)
}

fn save_server(name: &str, options: &CommandLineOptions, locate: ServerFileLocator) -> Result<()> {
    save_server_at(&path_of_server_file(name, locate)?, options)
}

fn save_server_at(path: &Path, options: &CommandLineOptions) -> Result<()> {
    save_stats_endpoint(
        path,
        &StatsEndpoint {
            host: Some(options.host().to_string()),
            password: Some(options.password().to_string()),
        },
    )
    .with_context(|| format!("could not save {}", path.display()))?;
    eprintln!("Saved the stats connection in {}.", path.display());

    Ok(())
}

/// Reads the options and the server file they name.
pub(crate) fn parse() -> Result<CommandLineOptions> {
    resolve(CommandLineOptions::parse(), &server_file)
}

fn resolve(cli: CommandLineOptions, locate: ServerFileLocator) -> Result<CommandLineOptions> {
    let endpoint = match cli.server.as_deref() {
        Some(name) => {
            let path = open_server_file(name, locate)?;
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
        save_server(&name, &options, locate)?;
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
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use clap::Parser;
    use ql_zmq::servers::{StatsEndpoint, stats_endpoint};

    use super::{
        CommandLineOptions, open_existing, open_server_file, resolve, save_server, save_server_at,
    };
    use crate::format::{ColorChoice, OutputFormat};

    fn server_file_path(test: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ql_stats_cmd_line_{test}_{}", std::process::id()))
            .join("crimson-arena.toml")
    }

    fn remove_directory_of(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

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

    #[test]
    fn a_missing_server_file_is_reported_with_the_way_to_create_it() {
        let path = server_file_path("missing");

        let message = open_existing("crimson-arena", path)
            .expect_err("the file does not exist")
            .to_string();

        assert!(message.contains("--save-server crimson-arena"), "{message}");
    }

    #[test]
    fn an_existing_server_file_is_opened() {
        let path = server_file_path("existing");
        let options = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");
        save_server_at(&path, &options).expect("could not save the server");

        let opened = open_existing("crimson-arena", path.clone());

        assert_eq!(opened.expect("the file exists"), path);
        remove_directory_of(&path);
    }

    #[test]
    fn a_saved_connection_is_read_back_from_the_server_file() {
        let path = server_file_path("saved");
        let options = CommandLineOptions::try_parse_from([
            "ql_stats",
            "--host",
            "192.0.2.1:27962",
            "--password",
            "stats",
        ])
        .expect("the options are valid")
        .with_server_file(StatsEndpoint::default());

        save_server_at(&path, &options).expect("could not save the server");

        assert_eq!(
            stats_endpoint(&path).expect("could not read the server file"),
            endpoint(Some("tcp://192.0.2.1:27962"), Some("stats"))
        );
        remove_directory_of(&path);
    }

    #[test]
    fn invalid_server_names_are_rejected_before_the_file_system_is_touched() {
        let options = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");

        for result in [
            open_server_file("../crimson-arena", &|_| None).map(|_| ()),
            save_server("../crimson-arena", &options, &|_| None),
            resolve(
                CommandLineOptions::try_parse_from(["ql_stats", "../crimson-arena"])
                    .expect("the options are valid"),
                &|_| None,
            )
            .map(|_| ()),
        ] {
            let message = result.expect_err("the name is invalid").to_string();
            assert!(message.contains("not a valid server name"), "{message}");
        }
    }

    fn resolved(args: &[&str], path: &Path) -> anyhow::Result<CommandLineOptions> {
        let cli = CommandLineOptions::try_parse_from(args).expect("the options are valid");
        let path = path.to_path_buf();
        resolve(cli, &move |_| Some(path.clone()))
    }

    #[test]
    fn resolving_without_a_server_keeps_the_options() {
        let cli = CommandLineOptions::try_parse_from(["ql_stats", "--host", "192.0.2.1:27962"])
            .expect("the options are valid");

        let options = resolve(cli, &|_| None).expect("no server file is needed");

        assert_eq!(options.host(), "tcp://192.0.2.1:27962");
    }

    #[test]
    fn resolving_a_server_reads_its_file() {
        let path = server_file_path("resolve_read");
        let saved = CommandLineOptions::try_parse_from([
            "ql_stats",
            "--host",
            "192.0.2.1:27962",
            "--password",
            "stats",
        ])
        .expect("the options are valid")
        .with_server_file(StatsEndpoint::default());
        save_server_at(&path, &saved).expect("could not save the server");

        let options = resolved(&["ql_stats", "crimson-arena"], &path);

        remove_directory_of(&path);
        let options = options.expect("the server file is readable");
        assert_eq!(options.host(), "tcp://192.0.2.1:27962");
        assert_eq!(options.password(), "stats");
    }

    #[test]
    fn resolving_a_missing_server_file_names_the_way_to_create_it() {
        let path = server_file_path("resolve_missing");

        let message = resolved(&["ql_stats", "crimson-arena"], &path)
            .expect_err("the file does not exist")
            .to_string();

        assert!(message.contains("--save-server crimson-arena"), "{message}");
    }

    #[test]
    fn resolving_an_unreadable_server_file_names_the_file() {
        let path = server_file_path("resolve_unreadable");
        fs::create_dir_all(path.parent().expect("the path has a directory"))
            .expect("could not create the directory");
        fs::write(&path, "[stats\nhost = ").expect("could not write the file");

        let message = resolved(&["ql_stats", "crimson-arena"], &path)
            .expect_err("the file is not valid TOML")
            .to_string();

        remove_directory_of(&path);
        assert!(message.contains("could not read"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn resolving_a_server_file_that_others_can_read_still_works() {
        use std::os::unix::fs::PermissionsExt;

        let path = server_file_path("resolve_exposed");
        let saved = CommandLineOptions::try_parse_from(["ql_stats", "--password", "stats"])
            .expect("the options are valid");
        save_server_at(&path, &saved).expect("could not save the server");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        let options = resolved(&["ql_stats", "crimson-arena"], &path);

        remove_directory_of(&path);
        assert_eq!(options.expect("the file is readable").password(), "stats");
    }

    #[test]
    fn resolving_with_save_server_writes_the_connection() {
        let path = server_file_path("resolve_save");

        let options = resolved(
            &[
                "ql_stats",
                "--host",
                "192.0.2.1:27962",
                "--password",
                "stats",
                "--save-server",
                "crimson-arena",
            ],
            &path,
        );

        let saved = stats_endpoint(&path);
        remove_directory_of(&path);
        assert_eq!(
            options.expect("the connection is saved").host(),
            "tcp://192.0.2.1:27962"
        );
        assert_eq!(
            saved.expect("could not read the server file"),
            endpoint(Some("tcp://192.0.2.1:27962"), Some("stats"))
        );
    }

    #[test]
    fn a_connection_that_cannot_be_saved_is_reported() {
        let blocker = server_file_path("resolve_blocked");
        fs::create_dir_all(blocker.parent().expect("the path has a directory"))
            .expect("could not create the directory");
        fs::write(&blocker, "").expect("could not write the file");
        let options = CommandLineOptions::try_parse_from(["ql_stats"]).expect("defaults are valid");

        let message = save_server_at(&blocker.join("crimson-arena.toml"), &options)
            .expect_err("the parent is a file")
            .to_string();

        remove_directory_of(&blocker);
        assert!(message.contains("could not save"), "{message}");
    }
}
