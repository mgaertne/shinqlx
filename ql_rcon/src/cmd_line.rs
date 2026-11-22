use core::str::FromStr;
use std::io::IsTerminal;

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use termcolor::ColorChoice;

use crate::profile::{Connection, ServerFile, server_file};

const DEFAULT_HOST: &str = "tcp://127.0.0.1:27961";

/// QuakeLive rcon options
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Name of a server file `servers/<name>.toml` in the config directory; its host, password, identity and color are
    /// used where no option is given, and what the server reports is kept in it.
    server: Option<String>,
    /// ZMQ URI to connect to, `tcp://127.0.0.1:27961` by default.
    #[arg(long)]
    host: Option<String>,
    /// The ZMQ password; asked for when there is none in the options and the server file.
    #[arg(long)]
    password: Option<String>,
    /// Specify the socket identity. Random UUID used by default
    #[arg(long)]
    identity: Option<String>,
    /// Use color output: always, ansi, auto or never
    #[arg(long)]
    color: Option<ColorChoice>,
    /// Ask the server for its commands and cvars and keep them for the completion of the next start
    #[arg(long)]
    fetch_completions: bool,
    /// Save host, password, identity and color as `servers/<name>.toml` (readable for the owner only) and connect
    #[arg(long, value_name = "NAME")]
    save_server: Option<String>,
}

/// The settings of a session after the options, the server file and the defaults are put together.
#[derive(Debug, Clone)]
pub(crate) struct CommandLineOptions {
    pub(crate) host: String,
    pub(crate) password: String,
    pub(crate) identity: String,
    pub(crate) color: ColorChoice,
    pub(crate) fetch_completions: bool,
    /// The name of the server file that is used, if any.
    pub(crate) profile: Option<String>,
}

/// The options and whether the password still has to be asked for.
#[derive(Debug)]
struct Resolved {
    options: CommandLineOptions,
    ask_password: bool,
}

fn with_scheme(host: String) -> String {
    if host.contains("://") {
        host
    } else {
        format!("tcp://{host}")
    }
}

impl Cli {
    /// Options win over the server file, the server file wins over the defaults.
    fn resolve(self, server: Connection) -> Result<Resolved> {
        let color = match (self.color, server.color) {
            (Some(color), _) => color,
            (None, Some(color)) => ColorChoice::from_str(&color)
                .map_err(|_| anyhow!("unknown color '{color}' in the server file"))?,
            (None, None) => ColorChoice::Auto,
        };
        let password = self.password.or(server.password);

        Ok(Resolved {
            ask_password: password.is_none(),
            options: CommandLineOptions {
                host: with_scheme(
                    self.host
                        .or(server.host)
                        .unwrap_or_else(|| DEFAULT_HOST.to_string()),
                ),
                password: password.unwrap_or_default(),
                identity: self.identity.or(server.identity).unwrap_or_default(),
                color,
                fetch_completions: self.fetch_completions,
                profile: self.server,
            },
        })
    }
}

fn open_server_file(name: &str) -> Result<ServerFile> {
    let path = server_file(name).ok_or_else(|| {
        anyhow!("'{name}' is not a valid server name or there is no config directory")
    })?;
    if !path.exists() {
        bail!(
            "no server file {}; create it with --save-server {name} and the connection options",
            path.display()
        );
    }

    Ok(ServerFile::new(path))
}

fn save_server(name: &str, options: &CommandLineOptions) -> Result<()> {
    let path = server_file(name).ok_or_else(|| {
        anyhow!("'{name}' is not a valid server name or there is no config directory")
    })?;
    let file = ServerFile::new(path);
    file.save_connection(&Connection {
        host: Some(options.host.clone()),
        password: Some(options.password.clone()),
        identity: (!options.identity.is_empty()).then(|| options.identity.clone()),
        color: None,
    })
    .with_context(|| format!("could not save {}", file.path().display()))?;
    eprintln!("Saved the connection in {}.", file.path().display());

    Ok(())
}

/// Reads the options, the server file they name and, where needed, the password from the terminal.
pub(crate) fn parse() -> Result<CommandLineOptions> {
    let cli = Cli::parse();
    let save_as = cli.save_server.clone();

    let connection = match cli.server.as_deref() {
        Some(name) => {
            let file = open_server_file(name)?;
            if file.password_is_exposed() {
                eprintln!(
                    "Warning: {} holds a password and can be read by other users, restrict it with chmod 600.",
                    file.path().display()
                );
            }
            file.connection()
                .with_context(|| format!("could not read {}", file.path().display()))?
        }
        None => Connection::default(),
    };

    let Resolved {
        mut options,
        ask_password,
    } = cli.resolve(connection)?;
    if ask_password && std::io::stdin().is_terminal() {
        options.password = rpassword::prompt_password(format!(
            "rcon password for {} (empty for none): ",
            options.host
        ))
        .context("could not read the password")?;
    }
    if let Some(name) = save_as {
        save_server(&name, &options)?;
        options.profile = Some(name);
    }

    Ok(options)
}

#[cfg(test)]
impl CommandLineOptions {
    /// The options of a command line without server file, for the tests of the completion.
    pub(crate) fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Cli::try_parse_from(args)?;
        Ok(cli
            .resolve(Connection::default())
            .map_err(|err| clap::Error::raw(clap::error::ErrorKind::InvalidValue, err))?
            .options)
    }
}

#[cfg(test)]
mod cmd_line_tests {
    use clap::Parser;
    use termcolor::ColorChoice;

    use super::{Cli, CommandLineOptions};
    use crate::profile::Connection;

    fn resolved(args: &[&str], server: Connection) -> CommandLineOptions {
        Cli::try_parse_from(args)
            .expect("the options are valid")
            .resolve(server)
            .expect("the settings are valid")
            .options
    }

    #[test]
    fn defaults_connect_to_the_local_server() {
        let args = CommandLineOptions::try_parse_from(["ql_rcon"]).expect("defaults are valid");

        assert_eq!(args.host, "tcp://127.0.0.1:27961");
        assert_eq!(args.password, "");
        assert_eq!(args.identity, "");
        assert_eq!(args.color, ColorChoice::Auto);
        assert!(!args.fetch_completions);
        assert_eq!(args.profile, None);
    }

    #[test]
    fn options_override_the_defaults() {
        let args = CommandLineOptions::try_parse_from([
            "ql_rcon",
            "--host",
            "tcp://192.0.2.1:28960",
            "--password",
            "secret",
            "--identity",
            "me",
            "--color",
            "never",
        ])
        .expect("the options are valid");

        assert_eq!(args.host, "tcp://192.0.2.1:28960");
        assert_eq!(args.password, "secret");
        assert_eq!(args.identity, "me");
        assert_eq!(args.color, ColorChoice::Never);
    }

    #[test]
    fn unknown_color_choices_are_rejected() {
        assert!(CommandLineOptions::try_parse_from(["ql_rcon", "--color", "rainbow"]).is_err());
    }

    #[test]
    fn the_server_file_fills_what_the_options_leave_out() {
        let server = Connection {
            host: Some("192.0.2.1:27961".to_string()),
            password: Some("from file".to_string()),
            identity: Some("crimson-arena".to_string()),
            color: Some("never".to_string()),
        };

        let args = resolved(&["ql_rcon", "crimson-arena"], server);

        assert_eq!(args.host, "tcp://192.0.2.1:27961");
        assert_eq!(args.password, "from file");
        assert_eq!(args.identity, "crimson-arena");
        assert_eq!(args.color, ColorChoice::Never);
        assert_eq!(args.profile.as_deref(), Some("crimson-arena"));
    }

    #[test]
    fn options_win_over_the_server_file() {
        let server = Connection {
            host: Some("tcp://192.0.2.1:27961".to_string()),
            password: Some("from file".to_string()),
            ..Connection::default()
        };

        let args = resolved(
            &[
                "ql_rcon",
                "crimson-arena",
                "--password",
                "typed",
                "--host",
                "tcp://192.0.2.2:1",
            ],
            server,
        );

        assert_eq!(args.host, "tcp://192.0.2.2:1");
        assert_eq!(args.password, "typed");
    }

    #[test]
    fn the_password_is_asked_for_only_when_nobody_gave_one() {
        let ask = |args: &[&str], server: Connection| {
            Cli::try_parse_from(args)
                .expect("the options are valid")
                .resolve(server)
                .expect("the settings are valid")
                .ask_password
        };

        assert!(ask(&["ql_rcon"], Connection::default()));
        assert!(!ask(&["ql_rcon", "--password", ""], Connection::default()));
        assert!(!ask(
            &["ql_rcon", "crimson-arena"],
            Connection {
                password: Some(String::new()),
                ..Connection::default()
            }
        ));
    }

    #[test]
    fn an_unknown_color_in_the_server_file_is_an_error() {
        let result = Cli::try_parse_from(["ql_rcon", "crimson-arena"])
            .expect("the options are valid")
            .resolve(Connection {
                color: Some("rainbow".to_string()),
                ..Connection::default()
            });

        assert!(result.is_err());
    }
}
